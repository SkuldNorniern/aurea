//! The languages the user asked the system to show apps in.
//!
//! Tags come back in BCP 47 form (`ko-KR`, `en-US`, `zh-Hans-CN`), most
//! preferred first, so an app picks its UI language by walking the list and
//! taking the first one it has strings for.

/// The user's preferred UI languages, most preferred first. Empty when the
/// platform says nothing.
///
/// Windows reads the display languages from Settings, macOS and iOS the
/// Language & Region list, and Linux and the BSDs `LANGUAGE`, `LC_ALL`,
/// `LC_MESSAGES` and `LANG` the way gettext does.
pub fn locales() -> Vec<String> {
    #[cfg(all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    ))]
    {
        use std::env;
        from_env(|name| env::var(name).ok())
    }
    #[cfg(not(all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )))]
    {
        from_platform()
    }
}

/// The user's most preferred UI language, as [`locales`] lists it first.
pub fn locale() -> Option<String> {
    locales().into_iter().next()
}

#[cfg(not(all(
    unix,
    not(target_os = "macos"),
    not(target_os = "ios"),
    not(target_os = "android")
)))]
fn from_platform() -> Vec<String> {
    use std::os::raw::c_char;
    use std::str;

    use crate::ffi::ng_platform_locales;

    // Room for a handful of tags; a longer list asks again at its size.
    let mut buf = vec![0u8; 128];
    loop {
        let cap = i32::try_from(buf.len()).unwrap_or(i32::MAX);
        let need = unsafe { ng_platform_locales(buf.as_mut_ptr().cast::<c_char>(), cap) };
        let Ok(need) = usize::try_from(need) else {
            return Vec::new();
        };
        if need == 0 {
            return Vec::new();
        }
        if need <= buf.len() {
            buf.truncate(need);
            break;
        }
        buf.resize(need, 0);
    }
    buf.split(|&b| b == 0)
        .filter_map(|tag| str::from_utf8(tag).ok())
        .filter_map(normalize)
        .collect()
}

/// What gettext would pick from the environment `var` reads: `LANGUAGE`'s
/// list first, unless the locale is `C` or `POSIX`, then the locale itself.
#[cfg_attr(
    not(all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )),
    allow(dead_code)
)]
fn from_env(var: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let set = |name: &str| var(name).filter(|v| !v.is_empty());
    let Some(current) = set("LC_ALL")
        .or_else(|| set("LC_MESSAGES"))
        .or_else(|| set("LANG"))
    else {
        return Vec::new();
    };
    let current = normalize(&current);
    let mut tags = Vec::new();
    if current.is_some()
        && let Some(list) = set("LANGUAGE")
    {
        tags.extend(list.split(':').filter_map(normalize));
    }
    tags.extend(current);
    let mut seen = Vec::with_capacity(tags.len());
    tags.retain(|tag| {
        let new = !seen.contains(tag);
        if new {
            seen.push(tag.clone());
        }
        new
    });
    tags
}

/// `ko_KR.UTF-8@euro` or `ko-KR` as `ko-KR`; `None` for `C`, `POSIX` and
/// anything without a language.
fn normalize(tag: &str) -> Option<String> {
    let tag = tag.split(['.', '@']).next().unwrap_or("").trim();
    if tag.is_empty() || tag == "C" || tag == "POSIX" {
        return None;
    }
    Some(tag.replace('_', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn posix_names_become_bcp47_tags() {
        assert_eq!(normalize("ko_KR.UTF-8").as_deref(), Some("ko-KR"));
        assert_eq!(normalize("de_DE@euro").as_deref(), Some("de-DE"));
        assert_eq!(normalize("ja").as_deref(), Some("ja"));
        assert_eq!(normalize("zh-Hans-CN").as_deref(), Some("zh-Hans-CN"));
        assert_eq!(normalize("C.UTF-8"), None);
        assert_eq!(normalize("POSIX"), None);
        assert_eq!(normalize(""), None);
    }

    #[test]
    fn language_list_comes_before_the_locale() {
        let tags = from_env(env(&[("LANGUAGE", "ja:en_GB"), ("LANG", "ko_KR.UTF-8")]));
        assert_eq!(tags, ["ja", "en-GB", "ko-KR"]);
    }

    #[test]
    fn lc_all_wins_over_lang() {
        let tags = from_env(env(&[("LC_ALL", "fr_FR.UTF-8"), ("LANG", "ko_KR.UTF-8")]));
        assert_eq!(tags, ["fr-FR"]);
    }

    #[test]
    fn c_locale_ignores_language_as_gettext_does() {
        assert!(from_env(env(&[("LANGUAGE", "ja"), ("LANG", "C")])).is_empty());
    }

    #[test]
    fn repeats_are_listed_once() {
        let tags = from_env(env(&[("LANGUAGE", "ko_KR:en"), ("LANG", "ko_KR.UTF-8")]));
        assert_eq!(tags, ["ko-KR", "en"]);
    }

    #[test]
    fn platform_list_reads_without_panicking() {
        for tag in locales() {
            assert!(!tag.is_empty() && !tag.contains('_'));
        }
    }
}
