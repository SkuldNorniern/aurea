//! One running copy of a desktop app: a second launch hands its files to the first and leaves.

use std::collections::hash_map::RandomState;
use std::env;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{self, Path, PathBuf};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use super::{files_in, opened};
use crate::files::app_local_data_dir;

/// How long one launch waits on the other.
const PATIENCE: Duration = Duration::from_secs(2);
/// How long a launch tries to reach a copy that has the lock, which may be just starting.
const STARTING: Duration = Duration::from_secs(5);
/// How long one try to connect takes at most.
const CONNECT: Duration = Duration::from_millis(200);
/// The file a running copy leaves its port and token in.
const WORD: &str = "instance";

static LOCK: OnceLock<File> = OnceLock::new();

/// Whether this is the one running copy of the app. False means a copy was running and has this
/// launch's files, with [`crate::WindowEvent::OpenRequested`], so exit. Call it early in `main`,
/// after [`set_identity`](super::set_identity). When it cannot tell, it says true: two copies beat
/// none.
///
/// ```no_run
/// aurea::app::set_identity(aurea::identity!());
/// if !aurea::app::single_instance() {
///     return;
/// }
/// ```
pub fn single_instance() -> bool {
    let Some(dir) = app_local_data_dir() else {
        return true;
    };
    let paths = files_in(env::args_os().skip(1))
        .iter()
        .map(|path| absolute(path))
        .collect();
    allow_foreground();
    match claim(&dir) {
        Ok(Some(lock)) => {
            // held until the process ends, however it ends.
            let _ = LOCK.set(lock);
            if let Err(error) = serve(&dir.join(WORD), opened) {
                log::warn!("single instance: {error}");
            }
            true
        }
        Ok(None) => hand_over_soon(&dir.join(WORD), &paths).is_err(),
        Err(error) => {
            log::warn!("single instance: {error}");
            true
        }
    }
}

/// Takes the lock on being the running copy; `None` when another copy has it. The system lets go
/// when the holder ends, so a crash leaves nothing behind.
fn claim(dir: &Path) -> io::Result<Option<File>> {
    fs::create_dir_all(dir)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("instance.lock"))?;
    match lock.try_lock() {
        Ok(()) => Ok(Some(lock)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// [`hand_over`], giving a copy that has only just taken the lock time to start listening.
fn hand_over_soon(file: &Path, paths: &Vec<PathBuf>) -> io::Result<()> {
    let start = Instant::now();
    loop {
        let result = hand_over(file, paths);
        if result.is_ok() || start.elapsed() >= STARTING {
            return result;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// This launch is what the user just did, so it may pass the foreground on to the running copy.
#[cfg(windows)]
fn allow_foreground() {
    const ANY_PROCESS: u32 = u32::MAX;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn AllowSetForegroundWindow(process: u32) -> i32;
    }
    unsafe { AllowSetForegroundWindow(ANY_PROCESS) };
}

#[cfg(not(windows))]
fn allow_foreground() {}

/// `path` from anywhere: the other copy has its own working folder.
fn absolute(path: &Path) -> PathBuf {
    path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Gives `paths` to the copy `file` names; an error when there is none.
fn hand_over(file: &Path, paths: &Vec<PathBuf>) -> io::Result<()> {
    let text = fs::read_to_string(file)?;
    let (port, token) = text
        .split_once('\n')
        .and_then(|(port, token)| Some((port.parse::<u16>().ok()?, token)))
        .ok_or(io::ErrorKind::InvalidData)?;
    // Windows retries a closed port for seconds, and an old file can name one.
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, CONNECT)?;
    stream.set_read_timeout(Some(PATIENCE))?;
    stream.set_write_timeout(Some(PATIENCE))?;
    let mut message = format!("{token}\n");
    for path in paths {
        message.push_str(&path.to_string_lossy());
        message.push('\n');
    }
    stream.write_all(message.as_bytes())?;
    stream.shutdown(Shutdown::Write)?;
    // a port another program reuses after a crash answers nothing like this.
    let mut answer = String::new();
    stream.read_to_string(&mut answer)?;
    if answer == "ok" {
        Ok(())
    } else {
        Err(io::ErrorKind::InvalidData.into())
    }
}

/// Listens for later launches, whose files go to `deliver`, and leaves word in `file`.
fn serve(file: &Path, deliver: fn(Vec<PathBuf>)) -> io::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let token = token();
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    write_private(file, &format!("{}\n{token}", listener.local_addr()?.port()))?;
    thread::Builder::new()
        .name("aurea-single-instance".to_owned())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                if let Err(error) = receive(stream, &token, deliver) {
                    log::debug!("single instance: {error}");
                }
            }
        })?;
    Ok(())
}

/// Takes one launch's files, if it knows the token, and tells it they arrived.
fn receive(mut stream: TcpStream, token: &str, deliver: fn(Vec<PathBuf>)) -> io::Result<()> {
    stream.set_read_timeout(Some(PATIENCE))?;
    stream.set_write_timeout(Some(PATIENCE))?;
    let mut lines = BufReader::new(stream.try_clone()?).lines();
    if lines.next().transpose()?.as_deref() != Some(token) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    deliver(
        lines
            .map(|line| line.map(PathBuf::from))
            .collect::<io::Result<_>>()?,
    );
    stream.write_all(b"ok")
}

/// A token only someone who can read the file knows, so others on this machine cannot open files.
fn token() -> String {
    let word = || RandomState::new().build_hasher().finish();
    format!("{:016x}{:016x}", word(), word())
}

#[cfg(unix)]
fn write_private(file: &Path, text: &str) -> io::Result<()> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    let mut out = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(file)?;
    out.write_all(text.as_bytes())
}

#[cfg(not(unix))]
fn write_private(file: &Path, text: &str) -> io::Result<()> {
    fs::write(file, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;
    use std::sync::Mutex;

    static GOT: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

    fn got(paths: Vec<PathBuf>) {
        GOT.lock().expect("got").extend(paths);
    }

    fn file(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("aurea-instance-{}", process::id()));
        dir.join(name)
    }

    #[test]
    fn only_one_holds_the_lock_and_it_lets_go() {
        let dir = file("lock");
        let first = claim(&dir).expect("claim");
        assert!(first.is_some());
        assert!(claim(&dir).expect("claim").is_none(), "taken");
        drop(first);
        assert!(claim(&dir).expect("claim").is_some(), "free again");
    }

    #[test]
    fn a_launch_waits_for_a_copy_just_starting() {
        let word = file("soon");
        let late = word.clone();
        let starting = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            serve(&late, got).expect("serve");
        });
        let path = PathBuf::from("/late/a.doc");
        hand_over_soon(&word, &vec![path.clone()]).expect("hand over");
        starting.join().expect("thread");
        assert!(GOT.lock().expect("got").contains(&path));
    }

    #[test]
    fn no_copy_means_no_hand_over() {
        assert!(hand_over(&file("none"), &Vec::new()).is_err());
    }

    #[test]
    fn a_second_launch_hands_its_files_to_the_first() {
        let word = file("word");
        serve(&word, got).expect("serve");
        let path = PathBuf::from("/some where/a.doc");
        hand_over(&word, &vec![path.clone()]).expect("hand over");
        assert!(GOT.lock().expect("got").contains(&path));
    }

    #[test]
    fn a_wrong_token_opens_nothing() {
        let word = file("token");
        serve(&word, |_| panic!("opened")).expect("serve");
        let text = fs::read_to_string(&word).expect("word");
        let port = text.split_once('\n').expect("port").0;
        fs::write(&word, format!("{port}\nwrong")).expect("write");
        assert!(hand_over(&word, &vec![PathBuf::from("a")]).is_err());
    }

    #[test]
    fn a_stale_file_is_no_copy() {
        let word = file("stale");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let port = listener.local_addr().expect("port").port();
        drop(listener);
        fs::create_dir_all(word.parent().expect("dir")).expect("dir");
        fs::write(&word, format!("{port}\ntoken")).expect("write");
        assert!(hand_over(&word, &Vec::new()).is_err());
    }
}
