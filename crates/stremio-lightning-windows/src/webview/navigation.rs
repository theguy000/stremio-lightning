pub(crate) fn is_allowed_webview_navigation(app_url: &str, target_url: &str) -> bool {
    let target = target_url.trim();
    if target.eq_ignore_ascii_case("about:blank") {
        return true;
    }

    match (url_origin(app_url), url_origin(target)) {
        (Some(app_origin), Some(target_origin)) => app_origin == target_origin,
        _ => false,
    }
}

pub(crate) fn url_origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let scheme = url[..scheme_end].to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }

    let authority_start = scheme_end + 3;
    let authority = url[authority_start..]
        .split(['/', '?', '#'])
        .next()?
        .to_ascii_lowercase();
    if authority.is_empty() || authority.contains('@') {
        return None;
    }

    Some(format!("{scheme}://{authority}"))
}
