mod font;
mod render;
mod svg;
mod utils;
mod highlight;

use anyhow::Error;
use clap::Parser;
use font::{FontConfig, FontStyle};
use highlight::HighlightSetting;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about,version,long_about=None)]
struct Args {
    /// input text string
    #[arg(conflicts_with = "file")]
    text: Option<String>,

    /// input file
    #[arg(long,short, conflicts_with = "text")]
    file: Option<PathBuf>,

    /// output svg file path
    #[arg(short, long, default_value = "output.svg")]
    output: Option<PathBuf>,

    /// font
    #[arg(long )]
    font: Option<String>,

    /// font size
    #[arg(long, default_value_t = 64)]
    size: u32,

    /// svg fill mode or fill color
    #[arg(long, default_value = "none")]
    fill: String,

    /// font color
    #[arg(long, default_value = "#000")]
    color: String,

    /// letter space (em)
    #[arg(long, default_value_t = 0.1)]
    space: f32,

    /// highlight mode
    #[arg(long)]
    highlight: bool,

    /// highlight theme or path to theme
    #[arg(long, requires="highlight", default_value="base16-ocean.dark")]
    theme: Option<String>,

    /// list supported file types/syntax
    #[arg(long)]
    list_syntax: bool,

    /// list supported theme
    #[arg(long)]
    list_theme: bool,

    /// debug mode
    #[arg(short, long)]
    debug: bool,

    /// list installed fonts
    #[arg(long)]
    list_fonts: bool,
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
        println!("args: {:?}", args);
    }

    if args.list_fonts {
        let fonts = font::fonts();
        for name in fonts.iter() {
            println!("{}", name);
        }
        return Ok(());
    }

    let mut highight_setting = HighlightSetting::default();
    if let Some(theme) = args.theme {
        if highight_setting.get_theme(theme.as_str()).is_none() {
            highight_setting.add_theme("user-theme", theme);
            highight_setting.set_theme("user-theme");
        }
    }

    if args.list_syntax {
        for syntax in highight_setting.syntax_set.syntaxes() {
            println!("- {} (.{})",syntax.name, syntax.file_extensions.join(", ."));
        }
    }

    if args.list_theme {
        for theme in highight_setting.theme_set.themes.keys() {
            println!("- {} ",theme);
        }
    }

    if let Some(font) = args.font {

        let mut font_config = FontConfig::new(font,args.size,args.fill,args.color,args.debug)?;
        font_config.set_letter_space(args.space);

        if args.debug {
            println!("{:?}", font_config);
        }

        if let Some(text) = args.text {
            render::render_text_to_svg_file(
                &text,
                &mut font_config,
                args.output.unwrap(),
            );
            return Ok(());
        } else if let Some(file) = args.file {
            if args.highlight {
                render::render_file_highlight(
                    &file,
                    &mut font_config,
                    &highight_setting,
                    args.output.unwrap(),
                );
            }else{
                render::render_text_file_to_svg(
                    &file,
                    &mut font_config,
                    args.output.unwrap(),
                );
            }
            return Ok(());

        }
        return Ok(());
    }
    Ok(())
}
