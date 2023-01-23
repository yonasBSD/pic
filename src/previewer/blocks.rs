use crate::options::{Action, Options};
use crate::result::{Error, Result};
use crate::utils::{
    ansi_color, fit_in_bounds, move_cursor, move_cursor_up, pixel_is_transparent, resize, TermSize,
};
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, DynamicImage, ImageFormat};
use std::fs::File;
use std::io::{Read, Write};
use std::thread;
use std::time::Duration;

const ANSI_CLEAR: &str = "\x1b[m";
const TOP_BLOCK: &str = "\u{2580}";
const BOTTOM_BLOCK: &str = "\u{2584}";

fn write_color_block(stdout: &mut impl Write, block: &str, ansi_bg: &str, ansi_fg: &str) -> Result {
    stdout.write_all(format!("{ansi_bg}{ansi_fg}{block}{ANSI_CLEAR}").as_bytes())?;
    stdout.flush()?;
    Ok(())
}

/// This function should only print a 'ready to display' frame
fn display_frame(stdout: &mut impl Write, image: &DynamicImage, options: &Options) -> Result {
    let rgba = &image.to_rgba8();
    let term_size = TermSize::from_ioctl()?;

    move_cursor(stdout, options.x, options.y)?;
    let mut backgrounds = vec![[0; 4]; rgba.width() as usize];
    for (r, row) in rgba.enumerate_rows() {
        let is_bg = r % 2 == 0;

        for (c, pixel) in row.enumerate() {
            let overflow_cols = (c as u32) + options.x.unwrap_or(0) >= term_size.cols;

            if !overflow_cols {
                if is_bg {
                    backgrounds[c] = pixel.2 .0;
                } else {
                    let rgb_fg = pixel.2 .0;
                    let rgb_bg = backgrounds[c];

                    match (pixel_is_transparent(rgb_fg), pixel_is_transparent(rgb_bg)) {
                        (true, true) => write_color_block(stdout, " ", "", "")?,
                        (true, false) => {
                            let ansi_fg = ansi_color(rgb_bg, false);
                            write_color_block(stdout, TOP_BLOCK, "", &ansi_fg)?
                        }
                        (false, true) => {
                            let ansi_fg = ansi_color(rgb_fg, false);
                            write_color_block(stdout, BOTTOM_BLOCK, "", &ansi_fg)?;
                        }
                        (false, false) => {
                            let ansi_bg = ansi_color(rgb_bg, true);
                            let ansi_fg = ansi_color(rgb_fg, false);
                            write_color_block(stdout, BOTTOM_BLOCK, &ansi_bg, &ansi_fg)?;
                        }
                    }
                }
            }
        }

        if !is_bg {
            stdout.write_all(b"\n")?;
        } else {
            move_cursor(stdout, options.x, None)?;
        };
    }

    Ok(())
}

fn display_gif(stdout: &mut impl Write, buffer: &[u8], options: &Options) -> Result {
    let frames: Vec<(Duration, DynamicImage)> = GifDecoder::new(buffer)?
        .into_frames()
        .collect_frames()?
        .iter()
        .map(|frame| {
            let delay = Duration::from(frame.delay());
            let image = &DynamicImage::ImageRgba8(frame.buffer().to_owned());
            let (width, height) = (image.width(), image.height());
            let (cols, rows) =
                fit_in_bounds(width, height, options.cols, options.rows, options.upscale)
                    .unwrap_or_default();

            // when playing gif we need one free row at the bottom (rows - 1)
            (delay, resize(&image, cols, (rows - 1) * 2))
        })
        .collect();

    for (delay, frame) in frames {
        display_frame(stdout, &frame, options)?;
        thread::sleep(delay);
        move_cursor_up(stdout, frame.height() / 2 - 1)?;
    }

    Ok(())
}

fn display_image(stdout: &mut impl Write, buffer: &[u8], options: &Options) -> Result {
    let image = image::load_from_memory(buffer)?;
    let (width, height) = (image.width(), image.height());
    let (cols, rows) = fit_in_bounds(width, height, options.cols, options.rows, options.upscale)?;

    display_frame(stdout, &resize(&image, cols, rows * 2), options)
}

fn display(stdout: &mut impl Write, options: &Options) -> Result {
    let mut image = File::open(&options.path)?;
    let mut buffer = Vec::new();
    image.read_to_end(&mut buffer)?;

    // hiding cursor might leave cursor hidden
    // hide_cursor(stdout)?;
    match (options.gif_static, image::guess_format(&buffer)?) {
        (false, ImageFormat::Gif) => display_gif(stdout, &buffer, options)?,
        _ => display_image(stdout, &buffer, options)?,
    }
    // show_cursor(stdout)?;
    Ok(())
}

pub fn preview(stdout: &mut impl Write, options: &Options) -> Result {
    match options.action {
        Action::Display => display(stdout, options),
        _ => Err(Error::ActionSupport(format!(
            "Blocks doesn't support '{}', try '--help'",
            options.action
        ))),
    }
}
