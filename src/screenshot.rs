use anyhow::{Context, Result};
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use std::fs;
use std::path::Path;
use std::process::Command;

const CELL_W: f32 = 8.0;
const CELL_H: f32 = 16.0;
const FONT_SIZE: f32 = 13.0;

pub fn render_png(buffer: &Buffer, path: &Path) -> Result<()> {
    let svg_path = path.with_extension("svg");
    write_svg(buffer, &svg_path)?;
    convert_svg_to_png(&svg_path, path)?;
    let _ = fs::remove_file(&svg_path);
    Ok(())
}

fn write_svg(buffer: &Buffer, path: &Path) -> Result<()> {
    let w = buffer.area.width;
    let h = buffer.area.height;
    let width = w as f32 * CELL_W;
    let height = h as f32 * CELL_H;

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<style>text {{ font-family: Menlo, Monaco, "Courier New", monospace; font-size: {FONT_SIZE}px; }}</style>
<rect width="100%" height="100%" fill="#1e1e2e"/>
"##
    ));

    for y in 0..h {
        for x in 0..w {
            let cell = &buffer[(x, y)];
            let ch = escape_xml(cell.symbol());
            let fg = color_to_hex(cell.fg);
            let bg = color_to_hex(cell.bg);
            let px = x as f32 * CELL_W;
            let py = y as f32 * CELL_H;
            if bg != "#1e1e2e" {
                svg.push_str(&format!(
                    r#"<rect x="{px}" y="{py}" width="{CELL_W}" height="{CELL_H}" fill="{bg}"/>"#
                ));
            }
            if ch != " " {
                svg.push_str(&format!(
                    r#"<text x="{px}" y="{py}" dy="12" fill="{fg}">{ch}</text>"#
                ));
            }
        }
    }

    svg.push_str("</svg>\n");
    fs::write(path, svg).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn convert_svg_to_png(svg: &Path, png: &Path) -> Result<()> {
    let status = Command::new("rsvg-convert")
        .arg("-o")
        .arg(png)
        .arg(svg)
        .status()
        .context("run rsvg-convert (install: brew install librsvg)")?;
    if !status.success() {
        anyhow::bail!("rsvg-convert failed");
    }
    Ok(())
}

fn escape_xml(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn color_to_hex(color: Color) -> String {
    let (r, g, b) = match color {
        Color::Reset | Color::Black => (0, 0, 0),
        Color::Red => (243, 139, 168),
        Color::Green => (166, 227, 161),
        Color::Yellow => (249, 226, 175),
        Color::Blue => (137, 180, 250),
        Color::Magenta => (203, 166, 247),
        Color::Cyan => (148, 226, 213),
        Color::Gray => (166, 173, 200),
        Color::DarkGray => (88, 91, 112),
        Color::LightRed => (243, 139, 168),
        Color::LightGreen => (166, 227, 161),
        Color::LightYellow => (249, 226, 175),
        Color::LightBlue => (137, 180, 250),
        Color::LightMagenta => (203, 166, 247),
        Color::LightCyan => (148, 226, 213),
        Color::White => (205, 214, 244),
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(_) => (166, 173, 200),
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}
