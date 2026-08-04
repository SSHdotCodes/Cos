//! Markdown rendering for chat messages, ported from MarkdownText.swift.
//! Parses into blocks (paragraph / header / bullet / code / quote / rule) with
//! inline runs, then renders via GPUI divs + styled text.

use gpui::{prelude::FluentBuilder, *};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CosColors {
    pub text: Hsla,
    pub text_muted: Hsla,
    pub accent: Hsla,
    pub surface_raised: Hsla,
    pub surface_border: Hsla,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InlineStyle {
    Plain,
    Bold,
    Italic,
    BoldItalic,
    Code,
    Link(String),
}

#[derive(Debug, Clone)]
pub struct InlineRun {
    pub text: String,
    pub style: InlineStyle,
}

#[derive(Debug, Clone)]
pub enum MdBlock {
    Paragraph(Vec<InlineRun>),
    Header(u8, Vec<InlineRun>),
    Bullet(u8, Vec<InlineRun>, bool /* ordered */),
    Code(String),
    Quote(Vec<InlineRun>),
    Rule,
}

pub fn parse_markdown(source: &str) -> Vec<MdBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(source, options);
    let mut blocks: Vec<MdBlock> = Vec::new();
    let mut inline: Vec<InlineRun> = Vec::new();
    let mut text_buffer = String::new();
    let mut bold = false;
    let mut italic = false;
    let mut code_span = false;
    let mut link: Option<String> = None;

    let mut pending: Option<MdBlockKind> = None;
    let mut list_stack: Vec<(bool, u8)> = Vec::new(); // (ordered, depth)
    let mut in_code_block = false;
    let mut code_buffer = String::new();

    #[derive(Clone, Copy, PartialEq)]
    enum MdBlockKind {
        Paragraph,
        Header(u8),
        Quote,
        ListItem,
    }

    macro_rules! flush_text {
        () => {{
            if !text_buffer.is_empty() {
                let style = if code_span {
                    InlineStyle::Code
                } else if let Some(dest) = link.clone() {
                    InlineStyle::Link(dest)
                } else if bold && italic {
                    InlineStyle::BoldItalic
                } else if bold {
                    InlineStyle::Bold
                } else if italic {
                    InlineStyle::Italic
                } else {
                    InlineStyle::Plain
                };
                let text = std::mem::take(&mut text_buffer);
                match inline.last_mut() {
                    Some(last) if last.style == style => last.text.push_str(&text),
                    _ => inline.push(InlineRun { text, style }),
                }
            }
        }};
    }

    macro_rules! flush_block {
        () => {{
            flush_text!();
            if let Some(kind) = pending.take() {
                match kind {
                    MdBlockKind::Paragraph => {
                        if !inline.is_empty() {
                            blocks.push(MdBlock::Paragraph(std::mem::take(&mut inline)));
                        }
                    }
                    MdBlockKind::Header(level) => {
                        blocks.push(MdBlock::Header(level, std::mem::take(&mut inline)));
                    }
                    MdBlockKind::Quote => {
                        if !inline.is_empty() {
                            blocks.push(MdBlock::Quote(std::mem::take(&mut inline)));
                        }
                    }
                    MdBlockKind::ListItem => {
                        if !inline.is_empty() {
                            let (ordered, depth) = list_stack.last().copied().unwrap_or((false, 0));
                            blocks.push(MdBlock::Bullet(depth, std::mem::take(&mut inline), ordered));
                        }
                    }
                }
            }
        }};
    }

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    if pending.is_none() {
                        pending = Some(MdBlockKind::Paragraph);
                    }
                }
                Tag::Heading { level, .. } => {
                    flush_block!();
                    pending = Some(MdBlockKind::Header(heading_level_value(level)));
                }
                Tag::BlockQuote(_) => {
                    flush_block!();
                    pending = Some(MdBlockKind::Quote);
                }
                Tag::CodeBlock(_) => {
                    flush_block!();
                    in_code_block = true;
                    code_buffer.clear();
                }
                Tag::List(first) => {
                    list_stack.push((first.is_some(), list_stack.len() as u8));
                }
                Tag::Item => {
                    flush_block!();
                    pending = Some(MdBlockKind::ListItem);
                }
                Tag::Strong => bold = true,
                Tag::Emphasis => italic = true,
                Tag::Link { dest_url, .. } => link = Some(dest_url.to_string()),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if pending == Some(MdBlockKind::Paragraph) {
                        flush_block!();
                    }
                }
                TagEnd::Heading(_) => flush_block!(),
                TagEnd::BlockQuote(_) => flush_block!(),
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    blocks.push(MdBlock::Code(code_buffer.trim_end_matches('\n').to_string()));
                    code_buffer.clear();
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                }
                TagEnd::Item => flush_block!(),
                TagEnd::Strong => bold = false,
                TagEnd::Emphasis => italic = false,
                TagEnd::Link => link = None,
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    code_buffer.push_str(&text);
                } else {
                    text_buffer.push_str(&text);
                }
            }
            Event::Code(text) => {
                flush_text!();
                code_span = true;
                text_buffer.push_str(&text);
                flush_text!();
                code_span = false;
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    code_buffer.push('\n');
                } else {
                    text_buffer.push('\n');
                }
            }
            Event::Rule => {
                flush_block!();
                blocks.push(MdBlock::Rule);
            }
            _ => {}
        }
    }
    flush_block!();
    blocks
}

fn heading_level_value(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Renders parsed markdown blocks into a GPUI element matching the Swift
/// MarkdownText styling: .text bubble foreground, compact spacing, monospace
/// code chips with the raised surface background.
pub fn render_markdown(
    source: &str,
    colors: &CosColors,
    base_font_size: f32,
    line_height: f32,
) -> AnyElement {
    let blocks = parse_markdown(source);
    let mut container = div().flex().flex_col().gap(px(4.0)).w_full();
    let link_color = colors.accent;
    for block in &blocks {
        let element = match block {
            MdBlock::Paragraph(runs) => inline_row(runs, colors, base_font_size, base_font_size, link_color).into_any_element(),
            MdBlock::Header(level, runs) => {
                let size = match level {
                    1 => base_font_size + 8.0,
                    2 => base_font_size + 6.0,
                    3 => base_font_size + 4.0,
                    _ => base_font_size + 2.0,
                };
                let row = inline_row(runs, colors, size, size, link_color).pt(px(4.0));
                row.into_any_element()
            }
            MdBlock::Bullet(depth, runs, ordered) => {
                let marker = if *ordered { "1." } else { "•" };
                div()
                    .flex()
                    .flex_row()
                    .gap(px(6.0))
                    .pl(px((*depth as f32) * 12.0))
                    .child(
                        div()
                            .text_color(colors.text.opacity(0.5))
                            .text_size(px(base_font_size))
                            .child(marker.to_string()),
                    )
                    .child(
                        inline_row(runs, colors, base_font_size, base_font_size, link_color).flex_1(),
                    )
                    .into_any_element()
            }
            MdBlock::Code(code) => div()
                .w_full()
                .px(px(9.0))
                .py(px(7.0))
                .rounded(px(8.0))
                .bg(colors.surface_raised)
                .border_1()
                .border_color(colors.surface_border)
                .text_size(px(base_font_size - 1.5))
                .font_family("Menlo")
                .text_color(colors.text)
                .line_height(px(base_font_size + 2.0))
                .whitespace_normal()
                .child(code.clone())
                .into_any_element(),
            MdBlock::Quote(runs) => div()
                .flex()
                .flex_row()
                .gap(px(8.0))
                .child(div().w(px(3.0)).h_full().rounded(px(2.0)).bg(colors.surface_border))
                .child(
                    inline_row(runs, colors, base_font_size, base_font_size, link_color)
                        .flex_1()
                        .text_color(colors.text_muted),
                )
                .into_any_element(),
            MdBlock::Rule => div()
                .w_full()
                .h(px(1.0))
                .my(px(6.0))
                .bg(colors.surface_border)
                .into_any_element(),
        };
        container = container.child(element);
    }
    container.line_height(px(line_height)).into_any_element()
}

fn inline_row(
    runs: &[InlineRun],
    colors: &CosColors,
    size: f32,
    _line_height: f32,
    link_color: Hsla,
) -> Div {
    let mut styled_runs: Vec<(String, TextStyle)> = Vec::new();
    for run in runs {
        let mut style = TextStyle {
            color: colors.text,
            font_size: px(size),
            ..Default::default()
        };
        match &run.style {
            InlineStyle::Plain => {}
            InlineStyle::Bold => style.font_weight = FontWeight::SEMIBOLD,
            InlineStyle::Italic => style.font_style = FontStyle::Italic,
            InlineStyle::BoldItalic => {
                style.font_weight = FontWeight::SEMIBOLD;
                style.font_style = FontStyle::Italic;
            }
            InlineStyle::Code => {
                style.font_family = Some("Menlo".to_string());
                style.font_size = px(size - 1.0);
                style.background_color = Some(colors.surface_raised);
            }
            InlineStyle::Link(_) => {
                style.color = link_color;
                style.underline = Some(UnderlineStyle {
                    color: Some(link_color),
                    thickness: px(1.0),
                    wavy: false,
                });
            }
        }
        if let Some((last_text, last_style)) = styled_runs.last_mut() {
            if *last_style == style {
                last_text.push_str(&run.text);
                continue;
            }
        }
        styled_runs.push((run.text.clone(), style));
    }

    let mut row = div().flex().flex_row().flex_wrap().whitespace_normal();
    for (text, style) in styled_runs {
        let mut span = div().text_size(style.font_size).text_color(style.color);
        if let Some(family) = &style.font_family {
            if !family.is_empty() {
                span = span.font_family(family.clone());
            }
        }
        if let Some(background) = style.background_color {
            span = span.bg(background).rounded(px(3.0)).px(px(2.0));
        }
        if style.font_weight != FontWeight::NORMAL {
            span = span.font_weight(style.font_weight);
        }
        if style.font_style == FontStyle::Italic {
            span = span.italic();
        }
        if style.underline.is_some() {
            span = span.underline();
        }
        row = row.child(span.child(text));
    }
    row
}

// Minimal TextStyle carrier (GPUI's StyledText requires Highlights; we keep a
// light local struct to merge runs).
#[derive(Debug, Clone, PartialEq)]
struct TextStyle {
    color: Hsla,
    font_size: Pixels,
    font_weight: FontWeight,
    font_style: FontStyle,
    font_family: Option<String>,
    background_color: Option<Hsla>,
    underline: Option<UnderlineStyle>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: gpui::white(),
            font_size: px(13.0),
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
            font_family: None,
            background_color: None,
            underline: None,
        }
    }
}
