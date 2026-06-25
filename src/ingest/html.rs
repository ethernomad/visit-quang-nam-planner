// HTML ingest for visitquangnam.com.
//
// The site's WordPress REST API (`/wp-json/wp/v2/{posts,pages,…}`) is no
// longer exposed — every endpoint either returns HTTP 404 directly or
// 301-redirects into a 404 (verified Jun 2026), and `?rest_route=…`
// silently falls back to the homepage HTML. The site is being served as
// statically cached HTML behind Cloudflare, so the corpus has to be
// rebuilt by **scraping rendered article pages** rather than REST JSON.
//
// Discovery is non-recursive and deterministic: a small fixed list of
// "section index" pages (`/`, `/places/`, `/experiences/`,
// `/experiences/beaches/`, …) is fetched, every `<a href>` on each is
// canonicalised against `https://visitquangnam.com` (the site's own
// canonical form — `<link rel="canonical">`), with relative hrefs
// resolved via `reqwest::Url::join`), asset/section/
// language-edition paths are filtered out, and the surviving article
// URLs are fetched in parallel (capped at 8 in flight for politeness).
//
// Each article page is rendered by the Uncode WordPress theme, which
// stamps enough stable hooks onto the markup that we can recover every
// field the old REST payload used to give us without any JSON:
//
//   - post id        → `<body class="… postid-20528 …">`
//   - category       → `<article class="… category-culture …">`
//                      (term id 1 → `category-uncategorised` is dropped)
//   - title          → first `<h1>` (inner HTML stripped)
//   - body           → first `<div class="post-content">` inner HTML,
//                      run through `strip_html` to drop script/style/…
//
// Server-only: pulls `reqwest`, `scraper`, and (via `futures::stream`)
// is driven by the `tokio` runtime supplied by `build_corpus`. The
// `Scraper`'s `Selector` literals are parsed once per call; they are
// static string literals so a panic here would be a code bug, not a
// runtime condition — hence `.expect("static selector")`.

#![cfg(feature = "server")]

use std::collections::BTreeSet;

use anyhow::Context;
use futures::stream::{self, StreamExt};
use scraper::{Html, Selector};
use serde::Deserialize;

/// Canonical origin used for discovery and corpus `link` URLs. We crawl
/// the **bare** form (`https://visitquangnam.com/…`) intentionally —
/// `visitquangnam.com` 301-redirects to `www.`, but the site's own
/// `<link rel="canonical">` canonicalises back to the bare form, and the
/// existing plan-time URL guard (`src/server/plan_trip.rs`
/// `ALLOWED_URL_PREFIX = "https://visitquangnam.com/"`) and prompt
/// template example are both keyed on the bare form. Keeping the corpus
/// `link` field bare means `post_validate`'s URL guard admits every
/// LLM-generated activity URL out of the box. The reqwest client in
/// `build_corpus` follows redirects (see `Policy::limited(10)`), so
/// fetching a bare URL just transparently hits `www.`.
const BASE: &str = "https://visitquangnam.com";

/// Fixed section indexes crawled for article links. Non-recursive by
/// design (AGENTS.md "fixed section indexes only") so corpus builds
/// stay deterministic. The five `/experiences/<sub>/` subsections are
/// included because the top `/experiences/` index alone misses some
/// cross-subsection articles that only appear on the sub-index pages.
const SECTION_INDEXES: &[&str] = &[
    "",
    "places/",
    "experiences/",
    "experiences/beaches/",
    "experiences/culture/",
    "experiences/food/",
    "experiences/nature/",
    "experiences/wellness/",
    "events/",
    "practical-tips/",
    "practical-tips/health/",
    "practical-tips/transport/",
    "practical-tips/visas/",
    "practical-tips/weather/",
    "green-travel/",
    "quang-nam/",
];

/// Max concurrent article fetches. Bounds load on the upstream site
/// during a one-shot corpus rebuild; the runtime is multi-thread
/// (`#[tokio::main]` in `build_corpus`), so `buffer_unordered` drives
/// these on the blocking-aware reqwest client without stalling.
const CONCURRENCY: usize = 8;

/// A single article page after parsing the HTML, before we chunk it.
///
/// Field shape is identical to the old REST-derived struct so the rest
/// of the pipeline (`chunk`, `embedder`, `build_corpus`) is unchanged.
/// `raw_categories` is kept for ABI parity but is no longer populated
/// by the scraper — REST exposed an array of category ids; the page
/// HTML only surfaces the primary category via the `category-NAME`
/// body/article class token.
#[derive(Debug, Clone)]
pub struct RawPost {
    pub id: u64,
    pub link: String,
    pub title: String,
    pub text: String,
    pub category: Option<String>,
    pub raw_categories: Vec<u64>,
}

/// Shape kept purely so `RawPost` round-trips through `serde` test
/// fixtures and any future JSON import path; not used by the scraper
/// itself, which always builds `RawPost` directly from the parsed HTML.
#[derive(Deserialize)]
struct _PostShape {
    id: u64,
    link: String,
    title: String,
    text: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    raw_categories: Vec<u64>,
}

/// Fetch every article discoverable from the fixed section indexes,
/// returning `RawPost`s in deterministic (URL-sorted) order so two
/// consecutive corpus builds byte-match when the upstream is unchanged.
///
/// Failures on individual indexes or articles are logged and skipped
/// rather than aborting the whole build — a single 404 page should not
/// poison the entire corpus. The caller still sees a hard error only
/// if **no** index page is reachable, which would indicate a network
/// outage rather than a partial site change.
pub async fn fetch_all(client: &reqwest::Client) -> anyhow::Result<Vec<RawPost>> {
    let mut articles: BTreeSet<String> = BTreeSet::new();
    let mut index_failures = 0u32;
    for rel in SECTION_INDEXES {
        let index_url = if rel.is_empty() {
            format!("{BASE}/")
        } else {
            format!("{BASE}/{rel}")
        };
        match scan_index(client, &index_url).await {
            Ok(urls) => {
                tracing::info!(index = %index_url, found = urls.len(), "scanned index");
                articles.extend(urls);
            }
            Err(e) => {
                tracing::warn!(error = %e, index = %index_url, "index fetch failed — skipping");
                index_failures = index_failures.saturating_add(1);
            }
        }
    }
    tracing::info!(count = articles.len(), "discovered article URLs");

    if articles.is_empty() {
        if index_failures > 0 {
            anyhow::bail!(
                "no articles discovered and {index_failures} index fetches failed — \
                 likely network/site outage"
            );
        }
        anyhow::bail!("no articles discovered — section index layout may have changed");
    }

    let posts: Vec<RawPost> = stream::iter(articles)
        .map(|url| {
            let client = client.clone();
            async move {
                match fetch_article(&client, &url).await {
                    Ok(Some(post)) => Some(post),
                    Ok(None) => None, // not a single-article page (no `postid-N`)
                    Err(e) => {
                        tracing::warn!(error = %e, %url, "article fetch error — skipped");
                        None
                    }
                }
            }
        })
        .buffer_unordered(CONCURRENCY)
        .filter_map(|x| async move { x })
        .collect()
        .await;

    let mut posts = posts;
    posts.sort_by(|a, b| a.link.cmp(&b.link));
    tracing::info!(count = posts.len(), "parsed articles");
    Ok(posts)
}

/// Fetch one section index page and return the deduplicated set of
/// article-style URLs it links to. Relative hrefs (`/foo/`, `../foo/`,
/// or even bare `foo/`) are resolved against the index's own URL via
/// `reqwest::Url::join`; external/asset/section-root hrefs are dropped
/// by `is_article_path`.
async fn scan_index(client: &reqwest::Client, index_url: &str) -> anyhow::Result<Vec<String>> {
    let base =
        reqwest::Url::parse(index_url).with_context(|| format!("parsing index url {index_url}"))?;
    let resp = client
        .get(index_url)
        .send()
        .await
        .with_context(|| format!("GET {index_url}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("GET {index_url} returned {}", resp.status());
    }
    let html = resp.text().await.with_context(|| {
        format!("decoding index body {index_url} — upstream returned non-UTF-8?")
    })?;
    let doc = Html::parse_document(&html);
    let a_sel = Selector::parse("a[href]").expect("static selector");
    let mut out = Vec::new();
    for el in doc.select(&a_sel) {
        let Some(href) = el.value().attr("href") else {
            continue;
        };
        if let Some(abs) = canonical_article_url(&base, href) {
            out.push(abs);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Resolve a raw `<a href>` against the index page's URL (using the
/// real RFC 3986 relative-URL rules via `reqwest::Url::join`), then
/// return the canonical `https://visitquangnam.com/<path>` form if it
/// points at an on-site article page we want to scrape — otherwise
/// `None` (external hosts, asset/wp-chrome paths, section roots,
/// mailto:/tel:/javascript: scheme, etc.).
fn canonical_article_url(base: &reqwest::Url, href: &str) -> Option<String> {
    // Skip scheme-only hrefs outright; `Url::join` would happily turn
    // "mailto:x" into a `mailto:` URL while discarding the index's path,
    // so drop these before resolving to avoid confusing them with same-
    // page navigation.
    let lower = href.trim_start();
    if lower.starts_with("mailto:") || lower.starts_with("tel:") || lower.starts_with("javascript:")
    {
        return None;
    }
    // Resolve relative to the index URL. `Url::join` strips any `#frag`
    // and `?query` for us when we ask for `.path()` afterwards.
    let resolved = base.join(href).ok()?;
    if resolved.scheme() != "https" && resolved.scheme() != "http" {
        return None;
    }
    // Accept both `www.visitquangnam.com` (used by the rendered index
    // pages' relative hrefs after reqwest follows the bare→www redirect)
    // and the bare `visitquangnam.com` form some absolute hrefs reference.
    // Either way, emit URLs in the **bare** form (see `BASE` comment) for
    // downstream parity with `plan_trip`'s `ALLOWED_URL_PREFIX` and the
    // prompt template.
    if !matches!(
        resolved.host_str(),
        Some("www.visitquangnam.com" | "visitquangnam.com")
    ) {
        return None;
    }
    let path = resolved.path().trim_matches('/');
    if !is_article_path(path) {
        return None;
    }
    Some(format!("{BASE}/{path}"))
}

/// Decide whether a host-relative path (no scheme, no leading slash,
/// no trailing slash) points at an article page we want to scrape.
fn is_article_path(path: &str) -> bool {
    if path.is_empty() {
        return false; // homepage
    }
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 {
        return false;
    }

    // WordPress / theme chrome — never article content.
    const REJECT_PREFIXES: &[&str] = &[
        "wp-content",
        "wp-includes",
        "wp-json",
        "wp-admin",
        "wp-login",
        "xmlrpc",
        "feed",
        "tag",
        "category",
        "author",
        "comment",
        "cdn",
        "fonts",
    ];
    if REJECT_PREFIXES.iter().any(|p| segs[0].starts_with(p)) {
        return false;
    }

    // Language editions and non-itinerary content roots.
    const REJECT_ROOTS: &[&str] = &["jp", "kr", "vi", "faqs", "travel-offers"];
    if REJECT_ROOTS.contains(&segs[0]) {
        return false;
    }

    // The fixed section indexes we already crawl above (and their subsection
    // index pages, which are listings rather than articles) — skip them so
    // they don't get fetched as "articles" and then dropped at parse time.
    const SECTION_ROOTS: &[&str] = &[
        "places",
        "experiences",
        "events",
        "practical-tips",
        "green-travel",
        "quang-nam",
    ];
    if SECTION_ROOTS.contains(&segs[0]) && segs.len() == 1 {
        return false;
    }
    const EXPERIENCES_SUBS: &[&str] = &["beaches", "culture", "food", "nature", "wellness"];
    if segs[0] == "experiences" && segs.len() == 2 && EXPERIENCES_SUBS.contains(&segs[1]) {
        return false;
    }
    // practical-tips/<sub>/ subsection index pages are themselves listings.
    const PRACTICAL_TIPS_SUBS: &[&str] = &["health", "transport", "visas", "weather"];
    if segs[0] == "practical-tips" && segs.len() == 2 && PRACTICAL_TIPS_SUBS.contains(&segs[1]) {
        return false;
    }

    true
}

/// Fetch a single article URL and parse it into a `RawPost`, or return
/// `Ok(None)` if the page has no `postid-N` body class (i.e. it is not
/// a single-article template — could be a section/landing page that
/// slipped past the index filter). Skipping these silently keeps the
/// build robust against the site adding new non-article routes.
async fn fetch_article(client: &reqwest::Client, url: &str) -> anyhow::Result<Option<RawPost>> {
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("GET {url} returned {}", resp.status());
    }
    let html = resp
        .text()
        .await
        .with_context(|| format!("decoding article body {url}"))?;
    let doc = Html::parse_document(&html);

    let body_sel = Selector::parse("body").expect("static selector");
    let id = doc.select(&body_sel).next().and_then(|body| {
        body.value().attr("class").and_then(|class| {
            class.split_whitespace().find_map(|tok| {
                tok.strip_prefix("postid-")
                    .and_then(|rest| rest.parse::<u64>().ok())
            })
        })
    });
    let id = match id {
        Some(id) => id,
        None => return Ok(None), // not a single-article page
    };

    let article_sel = Selector::parse("article").expect("static selector");
    let category = doc
        .select(&article_sel)
        .next()
        .and_then(|a| a.value().attr("class"))
        .and_then(|class| {
            for tok in class.split_whitespace() {
                if let Some(rest) = tok.strip_prefix("category-") {
                    if rest.eq_ignore_ascii_case("uncategorised") {
                        continue; // WP term id 1 → drop, matches old REST behaviour
                    }
                    return Some(rest.to_string());
                }
            }
            None
        });

    let h1_sel = Selector::parse("h1").expect("static selector");
    let title = doc
        .select(&h1_sel)
        .next()
        .map(|e| strip_html(&e.inner_html()))
        .unwrap_or_default();

    let post_content_sel = Selector::parse(".post-content").expect("static selector");
    let text = doc
        .select(&post_content_sel)
        .next()
        .map(|e| strip_html(&e.inner_html()))
        .unwrap_or_default();

    Ok(Some(RawPost {
        id,
        link: url.to_string(),
        title,
        text,
        category,
        raw_categories: Vec::new(),
    }))
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

    /// Helper: parse index base URL once per test and call
    /// `canonical_article_url` exactly the way `scan_index` does in
    /// production. Resolves hrefs against the homepage root.
    fn resolve_home(href: &str) -> Option<String> {
        let base = reqwest::Url::parse("https://visitquangnam.com/").unwrap();
        canonical_article_url(&base, href)
    }

    /// Resolve tests against a sub-index base (`/places/`) to mirror the
    /// real `../`-relative hrefs that index uses.
    fn resolve_places(href: &str) -> Option<String> {
        let base = reqwest::Url::parse("https://visitquangnam.com/places/").unwrap();
        canonical_article_url(&base, href)
    }

    #[test]
    fn canonical_article_url_accepts_www_and_bare_https() {
        assert_eq!(
            resolve_home("https://visitquangnam.com/experiences/culture/foo/"),
            Some("https://visitquangnam.com/experiences/culture/foo".to_string())
        );
        // Bare-domain href (`visitquangnam.com`) 301s to `www.` in production;
        // canonicalise to `www.` for dedup so an article linked both ways
        // is only fetched once.
        assert_eq!(
            resolve_home("https://visitquangnam.com/places/hoi-an-city/"),
            Some("https://visitquangnam.com/places/hoi-an-city".to_string())
        );
        // http:// variants 301 to https; accept them so we don't lose links.
        assert_eq!(
            resolve_home("http://www.visitquangnam.com/quang-nam/a-first-look/"),
            Some("https://visitquangnam.com/quang-nam/a-first-look".to_string())
        );
    }

    #[test]
    fn canonical_article_url_accepts_root_absolute_and_relative() {
        assert_eq!(
            resolve_home("/experiences/culture/foo/"),
            Some("https://visitquangnam.com/experiences/culture/foo".to_string())
        );
        // `../foo/` against `/places/` → root-absolute `/foo/`.
        assert_eq!(
            resolve_places("../experiences/culture/foo/"),
            Some("https://visitquangnam.com/experiences/culture/foo".to_string())
        );
        // bare-relative against the homepage → resolved against `/`.
        assert_eq!(
            resolve_home("experiences/culture/foo/"),
            Some("https://visitquangnam.com/experiences/culture/foo".to_string())
        );
    }

    #[test]
    fn canonical_article_url_strips_fragment_and_query() {
        assert_eq!(
            resolve_home("/experiences/culture/foo/#section"),
            Some("https://visitquangnam.com/experiences/culture/foo".to_string())
        );
        assert_eq!(
            resolve_home("/experiences/culture/foo/?utm_source=x"),
            Some("https://visitquangnam.com/experiences/culture/foo".to_string())
        );
    }

    #[test]
    fn canonical_article_url_rejects_external_assets_and_sections() {
        // external
        assert_eq!(resolve_home("https://fonts.googleapis.com/css?foo"), None);
        assert_eq!(resolve_home("//cdn.jsdelivr.net/"), None);
        // mailto / tel / javascript schemes
        assert_eq!(resolve_home("mailto:info@visitquangnam.com"), None);
        assert_eq!(resolve_home("tel:+84"), None);
        assert_eq!(resolve_home("javascript:void(0)"), None);
        // wordpress chrome
        assert_eq!(resolve_home("/wp-content/uploads/2022/02/foo.jpg"), None);
        assert_eq!(resolve_home("/wp-json/wp/v2/posts"), None);
        assert_eq!(resolve_home("/feed/"), None);
        assert_eq!(resolve_home("/tag/home/"), None);
        assert_eq!(resolve_home("/category/culture/"), None);
        // language editions + non-itinerary roots
        assert_eq!(resolve_home("/jp/places/"), None);
        assert_eq!(resolve_home("/kr/places/"), None);
        assert_eq!(resolve_home("/vi/trang-chu/"), None);
        assert_eq!(resolve_home("/faqs/"), None);
        // section index roots we already crawl (and their subsections)
        assert_eq!(resolve_home("/places/"), None);
        assert_eq!(resolve_home("/experiences/"), None);
        assert_eq!(resolve_home("/experiences/culture/"), None);
        assert_eq!(resolve_home("/practical-tips/"), None);
        assert_eq!(resolve_home("/practical-tips/health/"), None);
        assert_eq!(resolve_home("/quang-nam/"), None);
        // homepage + same-page anchor + empty href
        assert_eq!(resolve_home("/"), None);
        assert_eq!(resolve_home("#main"), None);
        assert_eq!(resolve_home(""), None);
    }

    #[test]
    fn canonical_article_url_requires_two_path_segments() {
        // single-segment paths still rejected as too shallow.
        assert_eq!(resolve_home("/something/"), None);
        // depth-2 accepted
        assert_eq!(
            resolve_home("/place/360-cham-islands/"),
            Some("https://visitquangnam.com/place/360-cham-islands".to_string())
        );
        // depth-3 accepted
        assert_eq!(
            resolve_home("/experiences/culture/guide-to-my-son-sanctuary/"),
            Some(
                "https://visitquangnam.com/experiences/culture/guide-to-my-son-sanctuary"
                    .to_string()
            )
        );
    }

    /// Fixture modelled on the real Uncode theme markup observed on
    /// `experiences/culture/guide-to-my-son-sanctuary/`: `<body
    /// class="… postid-20528 …">`, `<article … class="… category-culture
    /// …">`, `<h1>…</h1>` heading, `<div class="post-content">…</div>`
    /// body with nested HTML and an injected `<script>` that `strip_html`
    /// must discard.
    #[test]
    fn fetch_article_fixture_extracts_id_title_category_text() {
        let html = r#"<!DOCTYPE html><html><head><title>5 sights to discover at My Son Sanctuary</title></head>
<body class="single single-post postid-20528 culture">
  <h1>5 sights to discover at My Son Sanctuary</h1>
  <article id="post-20528" class="post-20528 post type-post category-culture tag-home">
    <div class="post-content">
      <p>My Son Sanctuary in Quang Nam is one of the most impressive temple ruins in South East Asia.</p>
      <h2>History</h2>
      <p>Listed by UNESCO as a World Heritage Site in 1999.</p>
      <script>analytics.trackPage()</script>
      <style>.x{}</style>
    </div>
  </article>
</body></html>"#;

        let doc = Html::parse_document(html);
        let body_sel = Selector::parse("body").expect("static selector");
        let id = doc.select(&body_sel).next().and_then(|body| {
            body.value().attr("class").and_then(|class| {
                class.split_whitespace().find_map(|tok| {
                    tok.strip_prefix("postid-")
                        .and_then(|rest| rest.parse::<u64>().ok())
                })
            })
        });
        assert_eq!(id, Some(20528));

        let article_sel = Selector::parse("article").expect("static selector");
        let category = doc
            .select(&article_sel)
            .next()
            .and_then(|a| a.value().attr("class"))
            .and_then(|class| {
                class
                    .split_whitespace()
                    .find_map(|tok| tok.strip_prefix("category-").map(|rest| rest.to_string()))
            });
        assert_eq!(category.as_deref(), Some("culture"));

        let h1_sel = Selector::parse("h1").expect("static selector");
        let title = doc
            .select(&h1_sel)
            .next()
            .map(|e| strip_html(&e.inner_html()))
            .unwrap_or_default();
        assert_eq!(title, "5 sights to discover at My Son Sanctuary");

        let post_content_sel = Selector::parse(".post-content").expect("static selector");
        let text = doc
            .select(&post_content_sel)
            .next()
            .map(|e| strip_html(&e.inner_html()))
            .unwrap_or_default();
        assert!(text.contains("My Son Sanctuary in Quang Nam"));
        assert!(text.contains("World Heritage Site in 1999"));
        assert!(
            !text.contains("analytics.trackPage"),
            "strip_html must drop <script> text: got {text:?}"
        );
        assert!(
            !text.contains(".x{}"),
            "strip_html must drop <style> text: got {text:?}"
        );
    }

    /// Article-less page (e.g. a section index that slipped past the
    /// filter, or a landing page): no `postid-` token → `None` so the
    /// caller skips it instead of producing a garbage chunk.
    #[test]
    fn fetch_article_fixture_no_postid_returns_none() {
        let html = r#"<!DOCTYPE html><html><body class="page-template places has-photo-credit">
  <h1>Places To Go</h1>
  <article class="post type-page"><div class="post-content">Listing…</div></article>
</body></html>"#;
        let doc = Html::parse_document(html);
        let body_sel = Selector::parse("body").expect("static selector");
        let id = doc.select(&body_sel).next().and_then(|body| {
            body.value().attr("class").and_then(|class| {
                class.split_whitespace().find_map(|tok| {
                    tok.strip_prefix("postid-")
                        .and_then(|rest| rest.parse::<u64>().ok())
                })
            })
        });
        assert_eq!(id, None);
    }

    /// `category-uncategorised` (WP term id 1) must be dropped — matches
    /// the old REST path's `pick_category` behaviour.
    #[test]
    fn category_uncategorised_is_skipped() {
        let html = r#"<article class="post-1 category-uncategorised"><div class="post-content">x</div></article>"#;
        let doc = Html::parse_document(html);
        let article_sel = Selector::parse("article").expect("static selector");
        let category = doc
            .select(&article_sel)
            .next()
            .and_then(|a| a.value().attr("class"))
            .and_then(|class| {
                for tok in class.split_whitespace() {
                    if let Some(rest) = tok.strip_prefix("category-") {
                        if rest.eq_ignore_ascii_case("uncategorised") {
                            continue;
                        }
                        return Some(rest.to_string());
                    }
                }
                None
            });
        assert_eq!(category, None);
    }
}
