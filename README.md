# text2svg

A tool help to convert text to svg file

## Usage

```
Usage: text2svg [OPTIONS]

Options:
  -t, --text <TEXT>      input text string
  -o, --output <OUTPUT>  output svg file path [default: output.svg]
      --font <FONT>      font
      --size <SIZE>      font size [default: 64]
      --fill <FILL>      svg fill mode or fill color [default: none]
      --color <COLOR>    font color [default: #000]
      --space <SPACE>    letter space (em) [default: 0.1]
  -d, --debug            debug mode
      --list             list installed fonts
  -h, --help             Print help
  -V, --version          Print version
```

```
text2svg --font "Ubuntu Mono" --text "Hello,world >= <= -> <
- == != ===" --output "hello.svg" --space 0 --color red
```

## How it works

WIP

## Roadmap

- [x] Convert text to SVG
  - [x] Ligature support
- [x] Modifiy SVG structure to add style
- [x] Convert text file to SVG
- [ ] code highlight
- [ ] Export as a lib
