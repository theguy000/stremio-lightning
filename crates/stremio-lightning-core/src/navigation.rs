/// Schemes a shell may hand to the operating system when a link points outside
/// the app. Deliberately a union of every scheme either shell used to accept
/// before this list became shared, so no existing stream, IPFS or mail link
/// breaks on either platform.
const EXTERNAL_URL_SCHEMES: &[&str] = &[
    "http://", "https://", "mailto:", "rtp://", "rtsp://", "ftp://", "ipfs://",
];

/// Whether a main-frame navigation may stay inside the app webview.
///
/// Only `about:blank` and the app URL's own origin qualify. Everything else must
/// be cancelled and routed through [`validate_external_url`] + the system
/// handler, otherwise a remote page would render inside the shell with the
/// native bridge injected into it.
#[must_use]
pub fn is_allowed_webview_navigation(app_url: &str, target_url: &str) -> bool {
    let target = target_url.trim();
    if target.eq_ignore_ascii_case("about:blank") {
        return true;
    }

    match (url_origin_parts(app_url), url_origin_parts(target)) {
        (Some((app_scheme, app_authority)), Some((target_scheme, target_authority))) => {
            app_scheme.eq_ignore_ascii_case(target_scheme)
                && app_authority.eq_ignore_ascii_case(target_authority)
        }
        _ => false,
    }
}

/// Whether the app URL has no http(s) origin to compare against, which is the
/// case for `file://` developer smoke pages. Shells must then leave navigation
/// unrestricted instead of applying [`is_allowed_webview_navigation`], which
/// would reject every target.
#[must_use]
pub fn has_unrestricted_app_url(app_url: &str) -> bool {
    url_origin_parts(app_url).is_none()
}

/// # Errors
/// Returns an error when the URL is empty, contains control characters, or uses
/// a scheme outside the shared allowlist.
pub fn validate_external_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.contains(char::is_control) {
        return Err(REJECTED_EXTERNAL_URL.to_string());
    }

    if EXTERNAL_URL_SCHEMES.iter().any(|prefix| {
        trimmed
            .get(..prefix.len())
            .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
    }) {
        Ok(())
    } else {
        Err(REJECTED_EXTERNAL_URL.to_string())
    }
}

const REJECTED_EXTERNAL_URL: &str = "Rejected non-whitelisted open_external_url URL";

fn url_origin_parts(url: &str) -> Option<(&str, &str)> {
    let scheme_end = url.find("://")?;
    let scheme = &url[..scheme_end];
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }

    let authority = url[scheme_end + 3..].split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }

    Some((scheme, authority))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_is_limited_to_configured_origin() {
        let app_url = "https://web.stremio.com/#/";

        assert!(is_allowed_webview_navigation(
            app_url,
            "https://web.stremio.com/#/player"
        ));
        assert!(is_allowed_webview_navigation(app_url, "about:blank"));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "https://example.com/"
        ));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "file:///C:/test.html"
        ));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "javascript:alert(1)"
        ));
    }

    #[test]
    fn localhost_webview_origin_includes_port() {
        let app_url = "http://127.0.0.1:5173/";

        assert!(is_allowed_webview_navigation(
            app_url,
            "http://127.0.0.1:5173/player"
        ));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "http://127.0.0.1:11470/"
        ));
    }

    #[test]
    fn origin_comparison_ignores_case_and_rejects_userinfo() {
        let app_url = "https://web.stremio.com/";

        assert!(is_allowed_webview_navigation(
            app_url,
            "HTTPS://Web.Stremio.COM/"
        ));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "https://user@web.stremio.com/"
        ));
    }

    #[test]
    fn local_proxy_origin_allows_every_proxied_path() {
        let app_url = "http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/";

        assert!(is_allowed_webview_navigation(
            app_url,
            "http://127.0.0.1:11470/configure"
        ));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "https://web.stremio.com/"
        ));
    }

    #[test]
    fn non_http_app_urls_are_reported_as_unrestricted() {
        assert!(!has_unrestricted_app_url("https://web.stremio.com/"));
        assert!(has_unrestricted_app_url("file:///tmp/smoke.html"));
        assert!(has_unrestricted_app_url("stremio://addon.example/x.json"));
    }

    #[test]
    fn accepts_every_shared_external_scheme() {
        for url in [
            "http://example.com/",
            "https://example.com/",
            "mailto:someone@example.com",
            "rtp://example.com:5544",
            "rtsp://example.com:5544/stream",
            "ftp://example.com/file",
            "ipfs://bafybeigdyrzt",
            "HTTPS://EXAMPLE.COM/",
        ] {
            assert!(
                validate_external_url(url).is_ok(),
                "expected {url} to be allowed"
            );
        }
    }

    #[test]
    fn rejects_empty_control_and_non_allowlisted_urls() {
        for url in [
            "",
            "   ",
            "java\nscript:alert(1)",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "stremio://addon.example/x.json",
        ] {
            assert!(
                validate_external_url(url).is_err(),
                "expected {url} to be rejected"
            );
        }
    }
}
