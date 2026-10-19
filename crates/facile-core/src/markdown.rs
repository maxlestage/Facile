//! `markdown(text)`: Markdown as a view, for documentation and articles.
//!
//! Headings (with anchors), paragraphs, emphasis, code (Rythme code is
//! colored), links, images, lists, quotes, tables and rules. HTML written in
//! the text is shown as text: a page never runs what its Markdown holds.

use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::Value;

use crate::host;
use crate::json::escape_html;
use crate::util::{self, text_arg};

/// The anchor of a heading: `Les routes` gives `les-routes`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let c = match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Inline Markdown: code, emphasis, links, images.
fn inline(s: &str, out: &mut String) {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let text = |a: usize, b: usize| -> String { chars[a..b].iter().collect() };
    let find = |from: usize, pat: &str| -> Option<usize> {
        let p: Vec<char> = pat.chars().collect();
        (from..chars.len()).find(|&j| chars[j..].starts_with(&p))
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if i + 1 < chars.len() && "\\`*_[]()#!|-".contains(chars[i + 1]) => {
                escape_html(&chars[i + 1].to_string(), out);
                i += 2;
            }
            '`' => match find(i + 1, "`") {
                Some(j) => {
                    out.push_str("<code>");
                    escape_html(&text(i + 1, j), out);
                    out.push_str("</code>");
                    i = j + 1;
                }
                None => {
                    out.push('`');
                    i += 1;
                }
            },
            '*' | '_' if i + 1 < chars.len() && chars[i + 1] == c => {
                let close: String = [c, c].iter().collect();
                match find(i + 2, &close) {
                    Some(j) if j > i + 2 => {
                        out.push_str("<strong>");
                        inline(&text(i + 2, j), out);
                        out.push_str("</strong>");
                        i = j + 2;
                    }
                    _ => {
                        escape_html(&close, out);
                        i += 2;
                    }
                }
            }
            '*' | '_' if i + 1 < chars.len() && !chars[i + 1].is_whitespace() && (i == 0 || !chars[i - 1].is_alphanumeric()) => {
                match find(i + 1, &c.to_string()) {
                    Some(j) if j > i + 1 && !chars[j - 1].is_whitespace() => {
                        out.push_str("<em>");
                        inline(&text(i + 1, j), out);
                        out.push_str("</em>");
                        i = j + 1;
                    }
                    _ => {
                        out.push(c);
                        i += 1;
                    }
                }
            }
            '!' | '[' => {
                let image = c == '!';
                let start = if image { i + 1 } else { i };
                if image && chars.get(start) != Some(&'[') {
                    out.push('!');
                    i += 1;
                    continue;
                }
                let close = find(start + 1, "](");
                let end = close.and_then(|k| find(k + 2, ")"));
                match (close, end) {
                    (Some(k), Some(e)) => {
                        let label = text(start + 1, k);
                        let url = text(k + 2, e);
                        let safe = !url.trim_start().to_lowercase().starts_with("javascript:");
                        if image {
                            out.push_str("<img src=\"");
                            if safe {
                                escape_html(&url, out);
                            }
                            out.push_str("\" alt=\"");
                            escape_html(&label, out);
                            out.push_str("\" loading=\"lazy\">");
                        } else {
                            out.push_str("<a href=\"");
                            if safe {
                                escape_html(&url, out);
                            }
                            out.push('"');
                            if url.starts_with("http") {
                                out.push_str(" rel=\"noopener\"");
                            }
                            out.push('>');
                            inline(&label, out);
                            out.push_str("</a>");
                        }
                        i = e + 1;
                    }
                    _ => {
                        out.push(c);
                        i += 1;
                    }
                }
            }
            c => {
                escape_html(&c.to_string(), out);
                i += 1;
            }
        }
    }
}

const KEYWORDS: &[&str] =
    &["fn", "if", "then", "else", "match", "end", "do", "type", "test", "use", "and", "or", "not", "true", "false", "unless", "as"];

/// Rythme code with its parts in `<span class="tok-...">`.
pub fn highlight_rythme(code: &str, out: &mut String) {
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    let span = |out: &mut String, class: &str, s: &str| {
        out.push_str("<span class=\"tok-");
        out.push_str(class);
        out.push_str("\">");
        escape_html(s, out);
        out.push_str("</span>");
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '#' {
            let end = (i..chars.len()).find(|&j| chars[j] == '\n').unwrap_or(chars.len());
            let s: String = chars[i..end].iter().collect();
            span(out, "comment", &s);
            i = end;
        } else if c == '"' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '"' {
                if chars[j] == '\\' {
                    j += 1;
                }
                j += 1;
            }
            let end = (j + 1).min(chars.len());
            let s: String = chars[i..end].iter().collect();
            span(out, "string", &s);
            i = end;
        } else if c == ':' && i + 1 < chars.len() && chars[i + 1].is_alphabetic() && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '?') {
                j += 1;
            }
            let s: String = chars[i..j].iter().collect();
            span(out, "symbol", &s);
            i = j;
        } else if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '.' || chars[j] == '_') {
                j += 1;
            }
            let s: String = chars[i..j].iter().collect();
            span(out, "number", &s);
            i = j;
        } else if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '?') {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let next = chars.get(j).copied();
            if KEYWORDS.contains(&word.as_str()) {
                span(out, "keyword", &word);
            } else if word.chars().next().is_some_and(|f| f.is_uppercase()) {
                span(out, "type", &word);
            } else if next == Some('(') || (i > 0 && chars[i - 1] == '.') {
                span(out, "function", &word);
            } else {
                escape_html(&word, out);
            }
            i = j;
        } else if "|>=-+*/<!%".contains(c) {
            let mut j = i;
            while j < chars.len() && "|>=-+*/<!%".contains(chars[j]) {
                j += 1;
            }
            let s: String = chars[i..j].iter().collect();
            span(out, "operator", &s);
            i = j;
        } else {
            escape_html(&c.to_string(), out);
            i += 1;
        }
    }
}

fn is_rule(l: &str) -> bool {
    let t = l.trim();
    t.len() >= 3 && (t.chars().all(|c| c == '-') || t.chars().all(|c| c == '*') || t.chars().all(|c| c == '_'))
}

fn list_item(l: &str) -> Option<(bool, &str)> {
    let t = l.trim_start();
    for p in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(p) {
            return Some((false, rest));
        }
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        if let Some(rest) = t[digits..].strip_prefix(". ") {
            return Some((true, rest));
        }
    }
    None
}

fn cells(l: &str) -> Vec<String> {
    let t = l.trim().trim_start_matches('|').trim_end_matches('|');
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// Markdown as HTML.
pub fn to_html(md: &str) -> String {
    let lines: Vec<&str> = md.lines().collect();
    let mut out = String::with_capacity(md.len() * 2);
    let mut i = 0;
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, out: &mut String| {
        if !para.is_empty() {
            out.push_str("<p>");
            inline(&para.join(" "), out);
            out.push_str("</p>\n");
            para.clear();
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        if let Some(fence) = t.strip_prefix("```") {
            flush(&mut para, &mut out);
            let lang = fence.trim().to_string();
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1;
            let code = code.join("\n");
            out.push_str("<pre class=\"code\"");
            if !lang.is_empty() {
                out.push_str(" data-lang=\"");
                escape_html(&lang, &mut out);
                out.push('"');
            }
            out.push_str("><code>");
            if lang == "rythme" || lang == "ry" {
                highlight_rythme(&code, &mut out);
            } else {
                escape_html(&code, &mut out);
            }
            out.push_str("</code></pre>\n");
            continue;
        }
        if t.is_empty() {
            flush(&mut para, &mut out);
            i += 1;
            continue;
        }
        let level = t.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&level) && t[level..].starts_with(' ') {
            flush(&mut para, &mut out);
            let title = t[level..].trim();
            let id = slug(title);
            out.push_str(&format!("<h{level} id=\"{id}\">"));
            inline(title, &mut out);
            if level > 1 {
                out.push_str(&format!("<a class=\"anchor\" href=\"#{id}\" aria-label=\"lien\">#</a>"));
            }
            out.push_str(&format!("</h{level}>\n"));
            i += 1;
            continue;
        }
        if is_rule(t) {
            flush(&mut para, &mut out);
            out.push_str("<hr>\n");
            i += 1;
            continue;
        }
        if t.starts_with('>') {
            flush(&mut para, &mut out);
            let mut quote = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                quote.push(lines[i].trim_start().trim_start_matches('>').trim_start());
                i += 1;
            }
            out.push_str("<blockquote>");
            out.push_str(&to_html(&quote.join("\n")));
            out.push_str("</blockquote>\n");
            continue;
        }
        if t.starts_with('|') && i + 1 < lines.len() && lines[i + 1].trim().starts_with('|') && lines[i + 1].contains("--") {
            flush(&mut para, &mut out);
            let head = cells(t);
            let aligns: Vec<&str> = cells(lines[i + 1])
                .iter()
                .map(|c| match (c.starts_with(':'), c.ends_with(':')) {
                    (true, true) => "center",
                    (false, true) => "right",
                    _ => "",
                })
                .collect();
            out.push_str("<div class=\"table\"><table><thead><tr>");
            for (k, h) in head.iter().enumerate() {
                match aligns.get(k).filter(|a| !a.is_empty()) {
                    Some(a) => out.push_str(&format!("<th style=\"text-align: {a}\">")),
                    None => out.push_str("<th>"),
                }
                inline(h, &mut out);
                out.push_str("</th>");
            }
            out.push_str("</tr></thead><tbody>");
            i += 2;
            while i < lines.len() && lines[i].trim().starts_with('|') {
                out.push_str("<tr>");
                for (k, c) in cells(lines[i]).iter().enumerate() {
                    match aligns.get(k).filter(|a| !a.is_empty()) {
                        Some(a) => out.push_str(&format!("<td style=\"text-align: {a}\">")),
                        None => out.push_str("<td>"),
                    }
                    inline(c, &mut out);
                    out.push_str("</td>");
                }
                out.push_str("</tr>");
                i += 1;
            }
            out.push_str("</tbody></table></div>\n");
            continue;
        }
        if let Some((ordered, _)) = list_item(line) {
            flush(&mut para, &mut out);
            out.push_str(if ordered { "<ol>" } else { "<ul>" });
            while i < lines.len() {
                let Some((_, item)) = list_item(lines[i]) else { break };
                let mut text = item.to_string();
                i += 1;
                // Lines indented under an item continue it.
                while i < lines.len() && lines[i].starts_with("  ") && list_item(lines[i]).is_none() && !lines[i].trim().is_empty() {
                    text.push(' ');
                    text.push_str(lines[i].trim());
                    i += 1;
                }
                out.push_str("<li>");
                inline(&text, &mut out);
                out.push_str("</li>");
            }
            out.push_str(if ordered { "</ol>\n" } else { "</ul>\n" });
            continue;
        }
        para.push(t);
        i += 1;
    }
    flush(&mut para, &mut out);
    out
}

/// The headings of a text, for a table of contents: `[(level, title, anchor)]`.
pub fn headings(md: &str) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    let mut fenced = false;
    for l in md.lines() {
        let t = l.trim();
        if t.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let level = t.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&level) && t[level..].starts_with(' ') {
            let title = t[level..].trim().replace('`', "");
            out.push((level, title.clone(), slug(&title)));
        }
    }
    out
}

fn markdown(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let md = text_arg(a, 0, "markdown", span)?;
    Ok(util::record(vec![("raw", Value::text(&to_html(md)))]))
}

fn toc(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let md = text_arg(a, 0, "toc", span)?;
    Ok(Value::list(
        headings(md)
            .into_iter()
            .map(|(l, t, id)| util::record(vec![("level", Value::Int(l as i64)), ("title", Value::text(&t)), ("anchor", Value::text(&id))]))
            .collect(),
    ))
}

fn highlight(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let code = text_arg(a, 0, "highlight", span)?;
    let mut out = String::new();
    highlight_rythme(code, &mut out);
    Ok(util::record(vec![("raw", Value::text(&out))]))
}

pub static MARKDOWN: &[Builtin] = &[
    host!("markdown", "markdown(text)", 1, 1, markdown,
        "Markdown as a view: headings with anchors, emphasis, code (Rythme colored), links, lists, quotes, tables. HTML in the text is shown as text.",
        "Du Markdown en vue : titres avec ancres, emphase, code (Rythme coloré), liens, listes, citations, tableaux. Le HTML du texte est affiché comme du texte."),
    host!("toc", "toc(text)", 1, 1, toc,
        "The headings of a Markdown text: `[{level, title, anchor}]`, for a table of contents.",
        "Les titres d'un texte Markdown : `[{level, title, anchor}]`, pour une table des matières."),
    host!("highlight", "highlight(code)", 1, 1, highlight,
        "Rythme code, colored (`<span class=\"tok-keyword\">`...).", "Du code Rythme, coloré (`<span class=\"tok-keyword\">`...)."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders() {
        let html = to_html("# Titre\n\nUn *mot* et **deux** `code` [lien](/a).\n\n- un\n- deux\n\n```rythme\nfn f(x) = x + 1 # ok\n```\n\n| a | b |\n|---|--:|\n| 1 | 2 |\n\n<script>x</script>");
        assert!(html.contains("<h1 id=\"titre\">Titre</h1>"), "{html}");
        assert!(html.contains("<em>mot</em>") && html.contains("<strong>deux</strong>") && html.contains("<code>code</code>"), "{html}");
        assert!(html.contains("<a href=\"/a\">lien</a>"), "{html}");
        assert!(html.contains("<ul><li>un</li><li>deux</li></ul>"), "{html}");
        assert!(html.contains("<span class=\"tok-keyword\">fn</span>"), "{html}");
        assert!(html.contains("<span class=\"tok-comment\"># ok</span>"), "{html}");
        assert!(html.contains("<td style=\"text-align: right\">2</td>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(!to_html("[x](javascript:alert(1))").contains("javascript"));
        assert_eq!(slug("Les routes et `les` paramètres"), "les-routes-et-les-parametres");
    }
}
