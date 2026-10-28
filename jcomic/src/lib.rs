#![cfg_attr(not(test), no_std)]

use aidoku::{
    Chapter, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider, Manga,
    MangaPageResult, Page, PageContent, PageContext, Result, Source,
    alloc::{String, Vec, format, string::ToString},
    helpers::uri::{decode_uri, encode_uri_component},
    imports::{html::{Document, Element}, net::Request},
    prelude::*,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

const BASE: &str = "https://jcomic.net";
const PAGE_MARKER: &str = "?aidoku_page=";
const COVER_MARKER: &str = "?aidoku_cover=1";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";
struct JComic;

fn doc(url: &str) -> Result<Document> {
    let join = if url.contains('?') { '&' } else { '?' };
    let fresh_url = format!("{url}{join}aidoku_refresh={}", aidoku::imports::std::current_date());
    Ok(Request::get(fresh_url)?
        .header("User-Agent", USER_AGENT)
        .header("Cache-Control", "no-cache")
        .html()?)
}

fn image_request(url: String) -> Result<Request> {
    Ok(Request::get(url)?.header("Referer", "https://jcomic.net/")
        .header("User-Agent", USER_AGENT)
        .header("Accept", "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8"))
}

fn absolute(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") { url.to_string() }
    else if url.starts_with("//") { format!("https:{url}") }
    else if url.starts_with('/') { format!("{BASE}{url}") }
    else { format!("{BASE}/{url}") }
}

fn manga_url(slug: &str) -> String { format!("{BASE}/eps/{}", encode_uri_component(slug)) }
fn reader_url(slug: &str) -> String { format!("{BASE}/page/{}", encode_uri_component(slug)) }
fn chapter_url(slug: &str, number: &str) -> String {
    if number == "single" { reader_url(slug) }
    else { format!("{}/{number}", reader_url(slug)) }
}

// /page/series-slug[/number] -> series-slug
fn slug_from_url(href: &str) -> Option<&str> {
    let path = href.strip_prefix(BASE).unwrap_or(href);
    let rest = path.strip_prefix("/page/").or_else(|| path.strip_prefix("/eps/"))?;
    let slug = rest.split(['/', '?', '#']).next()?;
    if slug.is_empty() { None } else { Some(slug) }
}

fn first_attr(element: &Element, attrs: &[&str]) -> Option<String> {
    for attr in attrs {
        if let Some(s) = element.attr(attr) {
            if !s.is_empty() && !s.starts_with("data:") { return Some(absolute(&s)); }
        }
    }
    None
}

// Website JS: data-locked = "JCOMIC_TRAPS_" + reverse(base64(signedImageURL))
// Its image links are signed for ~60 seconds, so decode when Aidoku fetches
// each image instead of baking soon-to-expire links into the Page list.
fn decode_locked(value: &str) -> Result<String> {
    let data = value.strip_prefix("JCOMIC_TRAPS_").unwrap_or(value);
    let reversed: String = data.chars().rev().collect();
    let decoded = STANDARD.decode(reversed.as_bytes())
        .map_err(|_| aidoku::error!("JComic: could not base64 decode data-locked"))?;
    let url = String::from_utf8(decoded)
        .map_err(|_| aidoku::error!("JComic: invalid decoded image URL"))?;
    if !url.starts_with("https://") && !url.starts_with("http://") {
        bail!("JComic: decoded image URL did not have an HTTP scheme");
    }
    Ok(url)
}

impl Source for JComic {
    fn new() -> Self { Self }

    fn get_search_manga_list(
        &self, query: Option<String>, page: i32, _filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        let url = if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
            format!("{BASE}/search/{}/{page}", encode_uri_component(&q), page = page.max(1))
        } else { format!("{BASE}/cat/{}/{}", encode_uri_component("最近更新"), page.max(1)) };
        let html = doc(&url)?;
        let mut entries = Vec::new();
        // Search result blocks carry .comic-title with a /page/<slug> link.
        if let Some(cards) = html.select(".row.col-lg-4") {
            for card in cards {
                let Some(link) = card.select_first("a[href^='/page/'], a[href^='/eps/']") else { continue; };
                let Some(href) = link.attr("href") else { continue; };
                let Some(raw_slug) = slug_from_url(&href) else { continue; };
                let slug = decode_uri(raw_slug);
                if entries.iter().any(|m: &Manga| m.key == slug) { continue; }
                let title = card.select_first(".comic-title")
                    .and_then(|e| e.text())
                    .or_else(|| link.attr("title"))
                    .or_else(|| link.text())
                    .unwrap_or_default();
                if title.trim().is_empty() { continue; }
                let detail = if href.starts_with("/eps/") { manga_url(&slug) } else { reader_url(&slug) };
                let cover = Some(format!("{detail}{COVER_MARKER}"));
                entries.push(Manga { key: slug, title, cover,
                    url: Some(detail), ..Default::default() });
            }
        }
        let has_next_page = html.select(".pagination a").is_some_and(|links| links.into_iter().any(|a| {
            a.attr("href").and_then(|href| href.rsplit('/').next()?.parse::<i32>().ok()).is_some_and(|p| p > page.max(1))
        }));
        Ok(MangaPageResult { entries, has_next_page })
    }

    fn get_manga_update(&self, mut manga: Manga, needs_details: bool, needs_chapters: bool) -> Result<Manga> {
        let url = manga.url.clone().filter(|url| url.starts_with(BASE)).unwrap_or_else(|| manga_url(&manga.key));
        let html = doc(&url)?;
        if needs_details {
            manga.url = Some(url.clone());
            if let Some(title) = html.select_first("p.comic-title, .comic-title")
                .and_then(|e| e.text()) { manga.title = title; }
            manga.cover = Some(format!("{url}{COVER_MARKER}"));
            manga.description = html.select_first(".comic-date, .comic-intro")
                .and_then(|e| e.text());
        }
        if needs_chapters {
            let mut chapters: Vec<Chapter> = Vec::new();
            if let Some(links) = html.select("a[href^='/page/']") {
                for a in links {
                    let Some(href) = a.attr("href") else { continue; };
                    let path = href.trim_end_matches('/');
                    let Some(slug) = slug_from_url(path) else { continue; };
                    if decode_uri(slug) != manga.key { continue; }
                    let Some(number) = path.rsplit('/').next().and_then(|s| s.parse::<f32>().ok()) else { continue; };
                    let key = format!("{number}");
                    if chapters.iter().any(|ch| ch.chapter_number == Some(number)) { continue; }
                    let chapter_name = a.text().unwrap_or_default();
                    chapters.push(Chapter {
                        key: key.clone(), title: Some(if chapter_name.is_empty() { format!("第{key}话") } else { chapter_name }),
                        chapter_number: Some(number), url: Some(chapter_url(&manga.key, &key)),
                        ..Default::default()
                    });
                }
            }
            chapters.sort_by(|a,b| b.chapter_number.partial_cmp(&a.chapter_number).unwrap_or(core::cmp::Ordering::Equal));
            if chapters.is_empty() {
                chapters.push(Chapter { key: "single".into(), title: Some("全文".into()),
                    url: Some(reader_url(&manga.key)), ..Default::default() });
            }
            manga.chapters = Some(chapters);
        }
        Ok(manga)
    }

    fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        let url = chapter_url(&manga.key, &chapter.key);
        let html = doc(&url)?;
        let images = html.select("img.jcomic-img").ok_or_else(|| aidoku::error!("JComic reader markup missing"))?;
        let mut output = Vec::new();
        for (i, _) in images.into_iter().enumerate() {
            // Synthetic URL: the ImageRequestProvider will refresh short-lived signed links.
            output.push(Page { content: PageContent::url(format!("{url}{PAGE_MARKER}{i}")),
                ..Default::default() });
        }
        if output.is_empty() { bail!("JComic has no reader images for this chapter") }
        Ok(output)
    }
}

impl ImageRequestProvider for JComic {
    fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
        if let Some(detail) = url.strip_suffix(COVER_MARKER) {
            let html = doc(detail)?;
            let image = html.select_first("img.comic-thumb, img.jcomic-img")
                .ok_or_else(|| aidoku::error!("JComic cover missing"))?;
            let fresh = if let Some(locked) = image.attr("data-locked") { decode_locked(&locked)? }
                else { first_attr(&image, &["data-original", "data-src", "src"])
                    .ok_or_else(|| aidoku::error!("JComic cover URL missing"))? };
            return image_request(fresh);
        }
        if let Some((chapter, index_text)) = url.split_once(PAGE_MARKER) {
            let index = index_text.parse::<usize>().map_err(|_| aidoku::error!("Invalid reader image number"))?;
            let html = doc(chapter)?;
            let image = html.select("img.jcomic-img")
                .and_then(|items| items.into_iter().nth(index))
                .ok_or_else(|| aidoku::error!("JComic reader image no longer exists"))?;
            let image_url = if let Some(locked) = image.attr("data-locked") {
                decode_locked(&locked)?
            } else {
                first_attr(&image, &["data-original", "data-src", "src"])
                    .ok_or_else(|| aidoku::error!("JComic image has no URL"))?
            };
            return image_request(image_url);
        }
        image_request(url)
    }
}

impl DeepLinkHandler for JComic {
    fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
        let Some(raw_slug) = slug_from_url(&url) else { return Ok(None) };
        let slug = decode_uri(raw_slug);
        let path = url.strip_prefix(BASE).unwrap_or(&url);
        if path.starts_with("/page/") {
            let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
            if parts.len() >= 3 {
                return Ok(Some(DeepLinkResult::Chapter { manga_key: slug.into(), key: parts[2].into() }));
            }
        }
        Ok(Some(DeepLinkResult::Manga { key: slug.into() }))
    }
}

register_source!(JComic, ImageRequestProvider, DeepLinkHandler);

#[cfg(test)]
mod tests {
    use super::*;
    use aidoku_test::aidoku_test;

    #[aidoku_test]
    fn live_browse_search_chapters_and_page_urls() {
        let source = JComic::new();
        let browse = source.get_search_manga_list(None, 1, Vec::new()).unwrap();
        assert!(browse.entries.len() >= 10);
        assert!(browse.has_next_page);
        let second = source.get_search_manga_list(None, 2, Vec::new()).unwrap();
        assert!(!second.entries.is_empty());
        assert_ne!(browse.entries[0].key, second.entries[0].key);
        let found = source.get_search_manga_list(Some("恋狱岛".into()), 1, Vec::new()).unwrap();
        let manga = found.entries.into_iter().find(|m| m.title.contains("恋狱岛")).unwrap();
        let manga = source.get_manga_update(manga, true, true).unwrap();
        let chapters = manga.chapters.as_ref().unwrap();
        assert!(chapters.len() >= 42);
        let chapter = chapters.iter().find(|c| c.chapter_number == Some(40.0)).unwrap().clone();
        let pages = source.get_page_list(manga.clone(), chapter).unwrap();
        assert_eq!(pages.len(), 26);
        for index in [0, pages.len() - 1] {
            let PageContent::Url(url, _) = &pages[index].content else { panic!("expected image URL") };
            let request = source.get_image_request(url.clone(), None).unwrap();
            assert!(request.url().unwrap().starts_with("https://images.jcomic.net/"));
        }
        aidoku::println!("browse={}, chapters={}, pages={}", browse.entries.len(), chapters.len(), pages.len());
    }

    // The site's CDN rejects the Rust test runner with a Cloudflare 403, while
    // its real browser reader loads all 26 images. Keep this limitation visible.
    #[aidoku_test]
    #[ignore]
    fn live_cdn_image_response_requires_device_check() {
        let source = JComic::new();
        let url = format!("{}{COVER_MARKER}", manga_url("恋狱岛-极地恋爱-"));
        let response = source.get_image_request(url, None).unwrap().send().unwrap();
        assert_eq!(response.status_code(), 200);
        assert!(response.get_data().unwrap().len() > 1000);
    }

    #[aidoku_test]
    fn live_single_chapter_entry() {
        let source = JComic::new();
        let browse = source.get_search_manga_list(None, 1, Vec::new()).unwrap();
        let manga = browse.entries.into_iter().find(|m| m.url.as_ref().is_some_and(|u| u.contains("/page/"))).unwrap();
        let manga = source.get_manga_update(manga, true, true).unwrap();
        let chapters = manga.chapters.as_ref().unwrap();
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].key, "single");
        let chapter = chapters[0].clone();
        let pages = source.get_page_list(manga, chapter).unwrap();
        assert!(!pages.is_empty());
    }
}
