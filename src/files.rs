//! Handing files to the desktop: open one with its default application, or show it in the file
//! manager, and find the folders an app keeps its own files in.

use std::env;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{self, Path, PathBuf};
use std::process::{self, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "android")]
use crate::android;
#[cfg(target_os = "ios")]
use crate::ios;
use crate::{AureaError, AureaResult};

/// Opens `path` the way double-clicking it in the file manager would. On iOS, Quick Look shows it
/// over the app.
pub fn open_path(path: &Path) -> AureaResult<()> {
    let path = &launch_path(path);
    #[cfg(target_os = "ios")]
    {
        return ios::preview_path(path);
    }
    #[allow(unreachable_code)]
    if cfg!(windows) {
        spawn(Command::new("explorer.exe").arg(path))
    } else if cfg!(target_os = "macos") {
        spawn(Command::new("open").arg(path))
    } else if cfg!(all(
        unix,
        not(target_os = "android"),
        not(target_os = "ios")
    )) {
        spawn(Command::new("xdg-open").arg(path))
    } else {
        Err(AureaError::unsupported("open_path"))
    }
}

/// Shows the folder holding `path` in the file manager, with `path` selected where the file manager
/// can do that. On iOS the Files app opens at the folder, if it is in the app's Documents.
pub fn reveal_path(path: &Path) -> AureaResult<()> {
    let path = &launch_path(path);
    #[cfg(target_os = "ios")]
    {
        return ios::reveal_path(path);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let arg = format!("/select,\"{}\"", path.display());
        return spawn(Command::new("explorer.exe").raw_arg(arg));
    }
    #[cfg(target_os = "macos")]
    {
        return spawn(Command::new("open").arg("-R").arg(path));
    }
    #[cfg(all(
        unix,
        not(target_os = "macos"),
        not(target_os = "android"),
        not(target_os = "ios")
    ))]
    {
        let uri = file_uri(path);
        let shown = Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.freedesktop.FileManager1",
                "--object-path",
                "/org/freedesktop/FileManager1",
                "--method",
                "org.freedesktop.FileManager1.ShowItems",
            ])
            .arg(format!("['{uri}']"))
            .arg("")
            .status()
            .is_ok_and(|status| status.success());
        if shown {
            return Ok(());
        }
        let folder = path.parent().unwrap_or(path);
        return spawn(Command::new("xdg-open").arg(folder));
    }
    #[allow(unreachable_code)]
    {
        let _ = path;
        Err(AureaError::unsupported("reveal_path"))
    }
}

/// Opens `url` in the app that handles it: a web page in the browser, a `mailto:` link in the mail
/// app. Only `http`, `https` and `mailto` URLs are opened, so a link from a document or a terminal
/// cannot start a program through a scheme of its own.
///
/// # Errors
///
/// [`AureaError::InvalidUrl`] for any other scheme, or a URL with spaces or control characters,
/// and [`AureaError::ElementOperationFailed`] when nothing opens it.
pub fn open_url(url: &str) -> AureaResult<()> {
    if !openable_url(url) {
        return Err(AureaError::InvalidUrl);
    }
    #[cfg(target_os = "ios")]
    {
        return ios::open_url(url);
    }
    #[cfg(target_os = "android")]
    {
        return if android::open_url(url) {
            Ok(())
        } else {
            Err(AureaError::ElementOperationFailed)
        };
    }
    #[allow(unreachable_code)]
    if cfg!(windows) {
        // the URL protocol handler, not the shell: no command line to break out of.
        spawn(
            Command::new("rundll32.exe")
                .arg("url.dll,FileProtocolHandler")
                .arg(url),
        )
    } else if cfg!(target_os = "macos") {
        spawn(Command::new("open").arg(url))
    } else if cfg!(unix) {
        spawn(Command::new("xdg-open").arg(url))
    } else {
        Err(AureaError::unsupported("open_url"))
    }
}

/// Whether `url` is one [`open_url`] opens: `http`, `https` or `mailto`, then something, with no
/// spaces or control characters (which the URL spec has percent-escaped).
fn openable_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    let web = ["http", "https"]
        .iter()
        .any(|s| scheme.eq_ignore_ascii_case(s));
    let allowed = if web {
        rest.strip_prefix("//")
            .is_some_and(|host| !host.is_empty() && !host.starts_with('/'))
    } else {
        scheme.eq_ignore_ascii_case("mailto") && !rest.is_empty()
    };
    allowed && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The folder `app` keeps its settings in, for the current user: `%APPDATA%\app` on Windows,
/// `~/Library/Application Support/app` on macOS and iOS (inside the app's sandbox there) and
/// `$XDG_CONFIG_HOME/app` (or `~/.config/app`) elsewhere.
pub fn config_dir(app: &str) -> Option<PathBuf> {
    user_dir(app, "XDG_CONFIG_HOME", ".config")
}

/// The folder `app` keeps its data in, for the current user: the same as [`config_dir`] on
/// Windows, macOS and iOS, and `$XDG_DATA_HOME/app` (or `~/.local/share/app`) elsewhere.
pub fn data_dir(app: &str) -> Option<PathBuf> {
    user_dir(app, "XDG_DATA_HOME", ".local/share")
}

/// The folder `app` keeps large per-machine data in, such as checkpoints or thumbnails.
pub fn local_data_dir(app: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        env_path("LOCALAPPDATA").map(|base| base.join(app))
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        data_dir(app)
    } else {
        user_dir(app, "XDG_STATE_HOME", ".local/state")
    }
}

/// The folder `app` keeps caches in.
pub fn cache_dir(app: &str) -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        return android::cache_dir().map(|cache| cache.join(app));
    }
    #[allow(unreachable_code)]
    if cfg!(windows) {
        env_path("LOCALAPPDATA").map(|base| base.join(app).join("Cache"))
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        env_path("HOME").map(|home| home.join("Library").join("Caches").join(app))
    } else {
        user_dir(app, "XDG_CACHE_HOME", ".cache")
    }
}

/// The folder the user's own files go in: `~/Documents`. On iOS it is the app's Documents, which
/// the Files app shows when the app's Info.plist sets `UIFileSharingEnabled` and
/// `LSSupportsOpeningDocumentsInPlace`.
pub fn documents_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        return android::files_dir().map(|files| files.join("Documents"));
    }
    #[allow(unreachable_code)]
    if cfg!(windows) {
        env_path("USERPROFILE").map(|home| home.join("Documents"))
    } else {
        env_path("HOME").map(|home| home.join("Documents"))
    }
}

fn user_dir(app: &str, xdg: &str, home_relative: &str) -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        // the app's own private storage; `xdg` places mean nothing there
        let _ = (xdg, home_relative);
        return android::files_dir().map(|files| files.join(app));
    }
    #[allow(unreachable_code)]
    let base = if cfg!(windows) {
        env_path("APPDATA")
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        // an iOS app's HOME is its sandbox; only Library, Documents and tmp in it can be written.
        env_path("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        env_path(xdg).or_else(|| env_path("HOME").map(|home| home.join(home_relative)))
    };
    base.map(|base| base.join(app))
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(any(
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "android"),
        not(target_os = "ios")
    ),
    test
))]
fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// `path` made absolute, since file managers do not resolve against this process's directory and
/// a leading `-` reads as an option to `open` or `xdg-open`.
fn launch_path(path: &Path) -> PathBuf {
    let path = path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    if path.as_os_str().as_encoded_bytes().first() == Some(&b'-') {
        Path::new(".").join(path)
    } else {
        path
    }
}

fn spawn(command: &mut Command) -> AureaResult<()> {
    command
        .spawn()
        .map(drop)
        .map_err(|e| AureaError::PlatformError(e.raw_os_error().unwrap_or(-1)))
}

/// A source file's identity for [`FileCache`]: an entry stored under one stamp misses under another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// The modification time, `None` when the platform does not report one.
    pub modified: Option<SystemTime>,
    /// The length in bytes.
    pub len: u64,
}

impl Stamp {
    /// The stamp of the file at `path`.
    pub fn of(path: &Path) -> io::Result<Stamp> {
        let meta = fs::metadata(path)?;
        Ok(Stamp {
            modified: meta.modified().ok(),
            len: meta.len(),
        })
    }

    /// The modification time as nanoseconds since the UNIX epoch, 0 for none.
    fn nanos(self) -> u64 {
        self.modified
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| {
                u64::try_from(since.as_nanos()).unwrap_or(u64::MAX)
            })
    }
}

/// A folder of cached files keyed by string, kept under a byte budget. Entries are replaced
/// atomically and keep the [`Stamp`] they were stored with, so a changed source is a miss.
#[derive(Clone, Debug)]
pub struct FileCache {
    folder: PathBuf,
    max_bytes: u64,
}

/// Entry header: modified nanos, source length and payload length, as little-endian u64s.
const HEADER: usize = 24;

static TEMP_COUNT: AtomicU64 = AtomicU64::new(0);

impl FileCache {
    /// The cache `name` in `app`'s cache folder, holding at most `max_bytes`. The folder is made
    /// on the first [`put`](Self::put). `None` when there is no cache folder.
    pub fn new(app: &str, name: &str, max_bytes: u64) -> Option<Self> {
        cache_dir(app).map(|dir| Self::at(dir.join(name), max_bytes))
    }

    /// The cache in `folder`, holding at most `max_bytes`.
    pub fn at(folder: PathBuf, max_bytes: u64) -> Self {
        Self { folder, max_bytes }
    }

    /// The bytes stored under `key`, if they were stored with `stamp`.
    pub fn get(&self, key: &str, stamp: Stamp) -> Option<Vec<u8>> {
        let bytes = fs::read(self.path(key)).ok()?;
        let [modified, len, payload_len] = decode(&bytes)?;
        let payload = bytes.get(HEADER..)?;
        let whole = u64::try_from(payload.len()).ok() == Some(payload_len);
        (whole && modified == stamp.nanos() && len == stamp.len).then(|| payload.to_vec())
    }

    /// Stores `bytes` under `key` with `stamp`, then drops the oldest entries until the folder is
    /// within budget. The entry just written is never dropped.
    pub fn put(&self, key: &str, stamp: Stamp, bytes: &[u8]) -> io::Result<()> {
        fs::create_dir_all(&self.folder)?;
        let name = file_name(key);
        let temp = self.folder.join(format!(
            "{name}.{}.{}.tmp",
            process::id(),
            TEMP_COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        let path = self.folder.join(format!("{name}.bin"));
        let payload_len =
            u64::try_from(bytes.len()).map_err(|e| io::Error::new(ErrorKind::InvalidInput, e))?;
        let mut entry = encode(stamp, payload_len).to_vec();
        entry.extend_from_slice(bytes);
        let written = fs::write(&temp, &entry).and_then(|()| fs::rename(&temp, &path));
        if written.is_err() {
            let _ = fs::remove_file(&temp);
        }
        written?;
        self.prune(&path);
        Ok(())
    }

    /// Drops the entry under `key`, if there is one.
    pub fn remove(&self, key: &str) -> io::Result<()> {
        ignore_missing(fs::remove_file(self.path(key)))
    }

    /// Drops every file in the cache folder. The folder itself stays.
    pub fn clear(&self) -> io::Result<()> {
        for (_, _, path) in self.files() {
            ignore_missing(fs::remove_file(path))?;
        }
        Ok(())
    }

    fn path(&self, key: &str) -> PathBuf {
        self.folder.join(format!("{}.bin", file_name(key)))
    }

    /// The regular files in the folder as (modified, length, path). Empty when it cannot be read.
    fn files(&self) -> Vec<(SystemTime, u64, PathBuf)> {
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let meta = entry.metadata().ok().filter(fs::Metadata::is_file)?;
                Some((
                    meta.modified().unwrap_or(UNIX_EPOCH),
                    meta.len(),
                    entry.path(),
                ))
            })
            .collect()
    }

    /// Removes the oldest files until the folder fits the budget, sparing `keep`.
    fn prune(&self, keep: &Path) {
        let mut files = self.files();
        let mut total: u64 = files.iter().map(|file| file.1).sum();
        files.sort_by_key(|file| file.0);
        for (_, len, path) in files {
            if total <= self.max_bytes {
                break;
            }
            if path != keep && fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(len);
            }
        }
    }
}

/// The file name for `key`: its FNV-1a hash in hex. The hash is stable across builds and any key
/// is safe in a file name.
fn file_name(key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in key.as_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn encode(stamp: Stamp, payload_len: u64) -> [u8; HEADER] {
    let mut header = [0; HEADER];
    let (chunks, _) = header.as_chunks_mut::<8>();
    for (chunk, field) in chunks
        .iter_mut()
        .zip([stamp.nanos(), stamp.len, payload_len])
    {
        *chunk = field.to_le_bytes();
    }
    header
}

/// The header fields of an entry: modified nanos, source length, payload length. `None` when
/// the entry is shorter than a header.
fn decode(bytes: &[u8]) -> Option<[u64; 3]> {
    let (chunks, _) = bytes.get(..HEADER)?.as_chunks::<8>();
    let mut fields = [0; 3];
    for (field, chunk) in fields.iter_mut().zip(chunks) {
        *field = u64::from_le_bytes(*chunk);
    }
    Some(fields)
}

fn ignore_missing(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    /// A unique folder under the temp dir, removed when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let name = format!(
                "aurea-files-{}-{tag}-{}",
                process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            );
            Self(env::temp_dir().join(name))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn stamp(len: u64) -> Stamp {
        Stamp {
            modified: None,
            len,
        }
    }

    fn total(cache: &FileCache) -> u64 {
        cache.files().iter().map(|file| file.1).sum()
    }

    #[test]
    fn put_then_get_round_trips_and_a_new_stamp_misses() {
        let dir = Scratch::new("round");
        let cache = FileCache::at(dir.0.clone(), 1 << 20);
        assert_eq!(cache.get("a", stamp(3)), None);
        cache.put("a", stamp(3), b"abc").expect("put");
        assert_eq!(cache.get("a", stamp(3)), Some(b"abc".to_vec()));
        assert_eq!(cache.get("a", stamp(4)), None);
        let dated = Stamp {
            modified: Some(UNIX_EPOCH + Duration::from_secs(1)),
            len: 3,
        };
        assert_eq!(cache.get("a", dated), None);
        cache.put("a", stamp(4), b"new").expect("put");
        assert_eq!(cache.get("a", stamp(4)), Some(b"new".to_vec()));
        cache.put("empty", stamp(0), b"").expect("put");
        assert_eq!(cache.get("empty", stamp(0)), Some(Vec::new()));
    }

    #[test]
    fn stamp_follows_the_source_file() {
        let dir = Scratch::new("source");
        fs::create_dir_all(&dir.0).expect("dir");
        let src = dir.0.join("src.txt");
        fs::write(&src, b"one").expect("write");
        let first = Stamp::of(&src).expect("stamp");
        assert_eq!(first.len, 3);
        fs::write(&src, b"four").expect("write");
        assert_ne!(Stamp::of(&src).expect("stamp"), first);
    }

    #[test]
    fn truncated_or_garbage_entries_miss() {
        let dir = Scratch::new("torn");
        let cache = FileCache::at(dir.0.clone(), 1 << 20);
        cache.put("k", stamp(1), b"payload").expect("put");
        let path = cache.path("k");
        let whole = fs::read(&path).expect("read");
        for cut in [0, 5, HEADER, whole.len() - 1] {
            fs::write(&path, &whole[..cut]).expect("write");
            assert_eq!(cache.get("k", stamp(1)), None, "cut at {cut}");
        }
        fs::write(&path, b"garbage that is long enough to hold a header").expect("write");
        assert_eq!(cache.get("k", stamp(1)), None);
    }

    #[test]
    fn prune_keeps_the_budget_and_the_newest_entry() {
        let dir = Scratch::new("prune");
        let cache = FileCache::at(dir.0.clone(), 150);
        // 64 bytes on disk each with the header
        let body = [7; 40];
        for key in ["a", "b", "c"] {
            cache.put(key, stamp(1), &body).expect("put");
            // coarse modified times on some filesystems
            thread::sleep(Duration::from_millis(20));
        }
        assert!(total(&cache) <= 150);
        assert_eq!(cache.get("a", stamp(1)), None);
        assert_eq!(cache.get("b", stamp(1)), Some(body.to_vec()));
        assert_eq!(cache.get("c", stamp(1)), Some(body.to_vec()));

        // alone over budget: kept, everything older goes
        let big = [1; 200];
        cache.put("big", stamp(1), &big).expect("put");
        assert_eq!(cache.get("big", stamp(1)), Some(big.to_vec()));
        assert_eq!(cache.get("b", stamp(1)), None);
        assert_eq!(cache.get("c", stamp(1)), None);
        assert_eq!(total(&cache), 224);
    }

    #[test]
    fn awkward_keys_work() {
        let dir = Scratch::new("keys");
        let cache = FileCache::at(dir.0.clone(), 1 << 20);
        let keys = [
            "thumbs/a/b.png",
            "C:\\x:y",
            "émoji 🎈 名前",
            "../../escape",
            "",
        ];
        for key in keys {
            cache.put(key, stamp(1), key.as_bytes()).expect("put");
        }
        for key in keys {
            assert_eq!(
                cache.get(key, stamp(1)),
                Some(key.as_bytes().to_vec()),
                "{key}"
            );
        }
        for (_, _, path) in cache.files() {
            assert_eq!(path.parent(), Some(dir.0.as_path()));
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            assert!(name.len() == 20 && name.ends_with(".bin"), "{name}");
        }
    }

    #[test]
    fn remove_and_clear_drop_entries() {
        let dir = Scratch::new("drop");
        let cache = FileCache::at(dir.0.clone(), 1 << 20);
        cache.put("a", stamp(1), b"a").expect("put");
        cache.put("b", stamp(1), b"b").expect("put");
        cache.remove("a").expect("remove");
        assert_eq!(cache.get("a", stamp(1)), None);
        assert_eq!(cache.get("b", stamp(1)), Some(b"b".to_vec()));
        cache.clear().expect("clear");
        assert_eq!(cache.get("b", stamp(1)), None);
        cache.remove("missing").expect("remove missing");
        cache.clear().expect("clear with no folder");
    }

    #[test]
    fn new_puts_the_cache_under_the_name() {
        if let Some(cache) = FileCache::new("Aurea Test", "thumbs", 10) {
            assert!(cache.folder.ends_with("thumbs"));
        }
    }

    #[test]
    fn only_web_and_mail_urls_open() {
        for url in [
            "https://example.com/a?b=c#d",
            "HTTP://example.com",
            "mailto:me@example.com",
        ] {
            assert!(openable_url(url), "{url}");
        }
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ms-settings:",
            r"C:\Windows\notepad.exe",
            "https://",
            "https:///path",
            "https:example.com",
            "https://example.com/a b",
            "https://example.com/\n--flag",
            "mailto:",
            "-https://x",
        ] {
            assert!(!openable_url(url), "{url}");
        }
    }

    #[test]
    fn launched_paths_are_absolute() {
        let cwd = env::current_dir().expect("cwd");
        assert_eq!(launch_path(Path::new("-n")), cwd.join("-n"));
        assert_eq!(launch_path(Path::new("a-b")), cwd.join("a-b"));
        let root = cwd.ancestors().last().expect("root").join("-n");
        assert_eq!(launch_path(&root), root);
    }

    #[test]
    fn file_uri_escapes_what_a_uri_or_gvariant_string_cannot_hold() {
        assert_eq!(
            file_uri(Path::new("/home/me/My Files/it's #1.txt")),
            "file:///home/me/My%20Files/it%27s%20%231.txt"
        );
        assert_eq!(
            file_uri(Path::new("/tmp/caf\u{e9}")),
            "file:///tmp/caf%C3%A9"
        );
    }

    #[test]
    fn app_folders_end_in_the_app_name() {
        for dir in [config_dir("Aurea Test"), data_dir("Aurea Test")]
            .into_iter()
            .flatten()
        {
            assert!(dir.ends_with("Aurea Test"));
        }
    }

    #[test]
    fn local_and_cache_folders_end_in_the_app_name() {
        if let Some(dir) = local_data_dir("Aurea Test") {
            assert!(dir.ends_with("Aurea Test"));
        }
        if let Some(dir) = cache_dir("Aurea Test") {
            let app = if cfg!(windows) {
                dir.parent().unwrap_or(dir.as_path())
            } else {
                dir.as_path()
            };
            assert!(app.ends_with("Aurea Test"));
        }
    }

    #[test]
    fn windows_local_folders_sit_under_local_app_data() {
        if !cfg!(windows) {
            return;
        }
        let Some(local) = env_path("LOCALAPPDATA") else {
            return;
        };
        if let Some(dir) = local_data_dir("Aurea Test") {
            assert!(dir.starts_with(&local));
        }
        if let Some(dir) = cache_dir("Aurea Test") {
            assert!(dir.starts_with(&local));
            assert!(dir.ends_with(Path::new("Aurea Test").join("Cache")));
        }
    }
}
