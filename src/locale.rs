//! The languages the user asked the system to show apps in.

use aurea_foundation::lock;
use std::sync::Mutex;

/// A menu row every app has, so its name is the same across them. See [`standard_label`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StandardLabel {
    /// The File menu.
    File,
    /// The Edit menu.
    Edit,
    /// The View menu.
    View,
    /// The Window menu.
    Window,
    /// The Help menu.
    Help,
    /// A new document.
    New,
    /// Open a document.
    Open,
    /// Save it.
    Save,
    /// Save it elsewhere.
    SaveAs,
    /// Close it.
    Close,
    /// Leave the app.
    Quit,
    /// The app's settings.
    Settings,
    /// Take back the last change.
    Undo,
    /// Do it again.
    Redo,
    /// Move to the clipboard.
    Cut,
    /// Copy to the clipboard.
    Copy,
    /// Bring in from the clipboard.
    Paste,
    /// Remove it.
    Delete,
    /// Select everything.
    SelectAll,
    /// Look closer.
    ZoomIn,
    /// Look further.
    ZoomOut,
    /// One to one.
    ActualSize,
    /// All of it in view.
    FitToScreen,
    /// The whole screen.
    FullScreen,
    /// Down to the dock or taskbar.
    Minimize,
    /// About the app.
    About,
}

/// The name of a standard menu row in `locale`, such as `ko-KR`: English and Korean so far, and
/// English for any other language.
pub fn standard_label(label: StandardLabel, locale: &str) -> &'static str {
    use StandardLabel as L;
    let korean = locale.split(['-', '_']).next() == Some("ko");
    let (english, hangul) = match label {
        L::File => ("File", "파일"),
        L::Edit => ("Edit", "편집"),
        L::View => ("View", "보기"),
        L::Window => ("Window", "창"),
        L::Help => ("Help", "도움말"),
        L::New => ("New", "새로 만들기"),
        L::Open => ("Open", "열기"),
        L::Save => ("Save", "저장"),
        L::SaveAs => ("Save As", "다른 이름으로 저장"),
        L::Close => ("Close", "닫기"),
        L::Quit => ("Quit", "종료"),
        L::Settings => ("Settings", "설정"),
        L::Undo => ("Undo", "실행 취소"),
        L::Redo => ("Redo", "다시 실행"),
        L::Cut => ("Cut", "잘라내기"),
        L::Copy => ("Copy", "복사"),
        L::Paste => ("Paste", "붙여넣기"),
        L::Delete => ("Delete", "삭제"),
        L::SelectAll => ("Select All", "모두 선택"),
        L::ZoomIn => ("Zoom In", "확대"),
        L::ZoomOut => ("Zoom Out", "축소"),
        L::ActualSize => ("Actual Size", "실제 크기"),
        L::FitToScreen => ("Fit to Screen", "화면에 맞춤"),
        L::FullScreen => ("Full Screen", "전체 화면"),
        L::Minimize => ("Minimize", "최소화"),
        L::About => ("About", "정보"),
    };
    if korean { hangul } else { english }
}

/// The languages last seen, to tell a change when the app comes back.
static SEEN: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// Whether the languages differ from when this was last asked; false the first time.
#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(crate) fn changed() -> bool {
    moved_on(locales(), &mut lock(&SEEN))
}

fn moved_on(now: Vec<String>, seen: &mut Option<Vec<String>>) -> bool {
    let changed = seen.as_ref().is_some_and(|before| *before != now);
    *seen = Some(now);
    changed
}

/// The user's preferred UI languages, most preferred first.
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

#[cfg(target_os = "android")]
fn from_platform() -> Vec<String> {
    use crate::android::language_tags;

    language_tags()
}

#[cfg(not(any(
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    ),
    target_os = "android"
)))]
fn from_platform() -> Vec<String> {
    use std::os::raw::c_char;
    use std::str;

    use crate::ffi::ng_platform_locales;

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

    #[test]
    fn a_change_shows_once_and_not_the_first_time() {
        let mut seen = None;
        assert!(!moved_on(vec!["en".into()], &mut seen));
        assert!(!moved_on(vec!["en".into()], &mut seen));
        assert!(moved_on(vec!["ko".into()], &mut seen));
        assert!(!moved_on(vec!["ko".into()], &mut seen));
    }

    #[test]
    fn standard_labels_follow_the_language_not_the_region() {
        assert_eq!(
            standard_label(StandardLabel::FitToScreen, "ko-KR"),
            "화면에 맞춤"
        );
        assert_eq!(
            standard_label(StandardLabel::FitToScreen, "ko"),
            "화면에 맞춤"
        );
        assert_eq!(standard_label(StandardLabel::Copy, "en-US"), "Copy");
        assert_eq!(
            standard_label(StandardLabel::Copy, "kok-IN"),
            "Copy",
            "not Korean"
        );
    }

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
