// WordPress REST ingest for visitquangnam.com.
//
// Fetches every published post and page from
// `https://visitquangnam.com/wp-json/wp/v2/{posts,pages}`, strips HTML,
// resolves the primary category to a human-readable name, and returns
// ready-to-chunk `RawPost` records. Server-only: pulls in `reqwest` and
// `scraper`, both gated behind the `server` feature.
//
// We use the `_embed` query param so the response carries term objects too,
// but for the MVP we resolve category ids the simple way: a one-shot fetch
// to `/wp-json/wp/v2/categories` to build a `HashMap<u64, String>`. WP
// term id 1 is "Uncategorised" and is mapped to `None`.

#![cfg(feature = "server")]

use std::collections::HashMap;

use anyhow::Context;
use scraper::Html;
use serde::Deserialize;

const BASE: &str = "https://visitquangnam.com/wp-json/wp/v2";
const PER_PAGE: u32 = 100;

/// A single WordPress post/page after parsing the REST JSON, before we
/// chunk it. `text` is the cleaned (`content.rendered` with HTML stripped);
/// `excerpt` and `categories` are kept for downstream use.
#[derive(Debug, Clone)]
pub struct RawPost {
    pub id: u64,
    pub link: String,
    pub title: String,
    pub text: String,
    pub category: Option<String>,
    pub raw_categories: Vec<u64>,
}

#[derive(Deserialize)]
struct WpPost {
    id: u64,
    link: String,
    title: Rendered,
    content: Rendered,
    #[serde(default)]
    categories: Vec<u64>,
}

#[derive(Deserialize)]
struct Rendered {
    rendered: String,
}

#[derive(Deserialize)]
struct WpCategory {
    id: u64,
    name: String,
}

/// Fetch every published post and page from visitquangnam.com, return
/// them as `RawPost`s in deterministic fetch order (posts first, then
/// pages). Pagination follows the WP REST convention: keep requesting
/// `?page=N` until a page returns fewer than `per_page` items.
pub async fn fetch_all(client: &reqwest::Client) -> anyhow::Result<Vec<RawPost>> {
    let categories = fetch_categories(client).await?;
    tracing::info!(n = categories.len(), "fetched category index");

    let mut out = Vec::new();
    for endpoint in &["posts", "pages"] {
        let kind = *endpoint;
        let mut page: u32 = 1;
        loop {
            let url = format!("{BASE}/{kind}?per_page={PER_PAGE}&_embed&page={page}");
            let resp = client
                .get(&url)
                .send()
                .await
                .with_context(|| format!("GET {url}"))?;
            if !resp.status().is_success() {
                anyhow::bail!("GET {url} returned {}", resp.status());
            }
            let items: Vec<WpPost> = resp.json().await.with_context(|| {
                format!("decoding {kind} page {page} — WP REST shape may have changed")
            })?;
            let got = items.len();
            let mapped: Vec<RawPost> = items
                .into_iter()
                .map(|p| map_post(p, &categories))
                .collect();
            out.extend(mapped);
            if (got as u32) < PER_PAGE {
                break;
            }
            page += 1;
        }
        tracing::info!(kind, count = out.len(), "collected");
    }
    Ok(out)
}

async fn fetch_categories(client: &reqwest::Client) -> anyhow::Result<HashMap<u64, String>> {
    let mut map = HashMap::new();
    let mut page: u32 = 1;
    loop {
        let url = format!("{BASE}/categories?per_page={PER_PAGE}&page={page}");
        let resp = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("GET {url} returned {}", resp.status());
        }
        let items: Vec<WpCategory> = resp.json().await.with_context(|| {
            format!("decoding categories page {page} — WP REST shape may have changed")
        })?;
        let got = items.len();
        for c in items {
            map.insert(c.id, c.name);
        }
        if (got as u32) < PER_PAGE {
            break;
        }
        page += 1;
    }
    Ok(map)
}

fn map_post(p: WpPost, categories: &HashMap<u64, String>) -> RawPost {
    let title = strip_html(&p.title.rendered);
    let text = strip_html(&p.content.rendered);
    let category = pick_category(&p.categories, categories);
    RawPost {
        id: p.id,
        link: p.link,
        title,
        text,
        category,
        raw_categories: p.categories,
    }
}

/// Resolve a post's category ids to a single human-readable name. Skip WP
/// term id 1 (Uncategorised) and any unknown id; if multiple named
/// categories survive, return the first in the order WordPress gave them.
fn pick_category(ids: &[u64], categories: &HashMap<u64, String>) -> Option<String> {
    for id in ids {
        if *id == 1 {
            continue;
        }
        if let Some(name) = categories.get(id) {
            return Some(name.clone());
        }
    }
    None
}

/// Strip HTML and collapse whitespace, dropping `<script>`/`<style>`/
/// `<noscript>` text nodes. We use `scraper::Html::parse_fragment`
/// (lenient) and walk the ego-tree from the root, collecting
/// `Node::Text` whose parent element isn't a known non-content tag.
///
/// We deliberately stay off `ego_tree`'s named types so this crate
/// doesn't need to depend on `ego-tree` directly: scraper re-exports
/// the `Node` enum and exposes the `Html::tree` field, and every method
/// we call (`root`, `descendants`, `value`, `parent`, `as_element`,
/// `name`) is available on the inferred types without naming them in
/// signatures.
pub(crate) fn strip_html(html: &str) -> String {
    if html.trim().is_empty() {
        return String::new();
    }
    let frag = Html::parse_fragment(html);
    let mut buf = String::with_capacity(html.len());
    for node in frag.tree.root().descendants() {
        let Some(text) = node.value().as_text() else {
            continue;
        };
        let skip = node.parent().is_some_and(|parent| {
            parent
                .value()
                .as_element()
                .is_some_and(|el| matches!(el.name(), "script" | "style" | "noscript"))
        });
        if skip {
            continue;
        }
        buf.push_str(text);
        buf.push(' ');
    }
    collapse_whitespace(buf.trim())
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_drops_script_and_style() {
        let html = "<p>Hello <b>world</b>.</p><script>evil()</script><style>.x{}</style>";
        assert_eq!(strip_html(html), "Hello world .");
    }

    #[test]
    fn pick_category_skips_uncategorised() {
        let mut cats = HashMap::new();
        cats.insert(1, "Uncategorised".to_string());
        cats.insert(7, "Food".to_string());
        assert_eq!(
            pick_category(&[1, 7], &cats).as_deref(),
            Some("Food"),
            "id 1 is skipped, first named wins"
        );
        assert_eq!(pick_category(&[1], &cats), None);
        assert_eq!(pick_category(&[], &cats), None);
    }
}
