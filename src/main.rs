mod font;
mod render;
mod svg;

use anyhow::Error;
use clap::Parser;
use font::{FontConfig, FontStyle};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(author,about,version,long_about=None)]
struct Args {
    /// input text string
    #[arg(short, long, conflicts_with = "list")]
    text: Option<String>,

    /// output svg file path
    #[arg(short, long, conflicts_with = "list", default_value = "output.svg")]
    output: Option<PathBuf>,

    /// font
    #[arg(long, conflicts_with = "list")]
    font: Option<String>,

    /// font size
    #[arg(long, conflicts_with = "list", default_value_t = 64)]
    size: u32,

    /// svg fill mode or fill color
    #[arg(long, conflicts_with = "list", default_value = "none")]
    fill: String,

    /// font color
    #[arg(long, conflicts_with = "list", default_value = "#000")]
    color: String,

    /// letter space (em)
    #[arg(long, conflicts_with = "list", default_value_t = 0.1)]
    space: f32,

    /// debug mode
    #[arg(short, long)]
    debug: bool,

    /// list installed fonts
    #[arg(long)]
    list: bool,
}

fn main() {

    if let Err(e) = run() {
        eprintln!("error: {}", e);
    }
}

fn run() -> Result<(),Error> {
    let args = Args::parse();

    if args.debug {
        println!("debug: {:?}", args.debug);
        println!("text: {:?}", args.text);
        println!("output: {:?}", args.output);
        println!("font: {:?}", args.font);
        println!("font size: {:?}", args.size);
        println!("list fonts: {:?}", args.list);
    }

    if args.list {
        let fonts = font::fonts();
        for name in fonts.iter() {
            println!("{}", name);
        }
        return Ok(());
    } else if let Some(font) = args.font {

        let mut font_config = FontConfig::new(font,args.size,args.fill,args.color)?;
        font_config.set_debug(args.debug)
            .set_letter_space(args.space);

        if args.debug {
            println!("{:?}", font_config);
        }

        if let Some(text) = args.text {
            render::render_text_to_svg_file(
                &text,
                &font_config,
                args.output.unwrap(),
            );
            return Ok(());
        }
        return Ok(());
    }
    return Ok(());
}
