pub(crate) fn is_allowed_webview_navigation(app_url: &str, target_url: &str) -> bool {
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
