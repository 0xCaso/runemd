//! New file (Cmd+N): the file is born empty in the given folder. A name already in use is not touched.

use std::fs::OpenOptions;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// The name to create: no spaces at the ends, `.md` appended if missing. `None` if it is not a
/// valid name: empty, with "/" or starting with "." (no subfolders, no hidden files).
pub fn normalize(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return None;
    }
    let has_md = Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"));
    Some(if has_md {
        name.to_owned()
    } else {
        format!("{name}.md")
    })
}

/// Creates `dir/name` empty and gives its path. With `create_new` (O_EXCL) a file already there is never
/// truncated, not even in a race: `AlreadyExists` gives that path, content unchanged; a folder with that
/// name stays an `AlreadyExists` error.
pub fn create(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let name = normalize(name)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "invalid name"))?;
    let path = dir.join(name);
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(_) => Ok(path),
        // A folder with that name is not a file to open.
        Err(e) if e.kind() == ErrorKind::AlreadyExists && !path.is_dir() => Ok(path),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn adds_md_and_creates_an_empty_file() {
        let d = tempfile::tempdir().unwrap();
        let p = create(d.path(), "note").unwrap();
        assert_eq!(p, d.path().join("note.md"));
        assert_eq!(fs::read_to_string(&p).unwrap(), "");
    }

    #[test]
    fn keeps_an_existing_md_extension() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(create(d.path(), "a.md").unwrap(), d.path().join("a.md"));
        assert!(!d.path().join("a.md.md").exists());
        assert_eq!(create(d.path(), "B.MD").unwrap(), d.path().join("B.MD"));
    }

    #[test]
    fn an_existing_name_is_returned_untouched() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("a.md"), "# mine\n").unwrap();
        assert_eq!(create(d.path(), "a").unwrap(), d.path().join("a.md"));
        assert_eq!(create(d.path(), "a.md").unwrap(), d.path().join("a.md"));
        assert_eq!(
            fs::read_to_string(d.path().join("a.md")).unwrap(),
            "# mine\n"
        );
    }

    /// A folder with the name asked for is not a file to open: the field stays open (`App::create_file`).
    #[test]
    fn a_folder_with_that_name_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("notes.md")).unwrap();
        let e = create(d.path(), "notes").unwrap_err();
        assert_eq!(e.kind(), ErrorKind::AlreadyExists);
        assert!(d.path().join("notes.md").is_dir(), "the folder is not touched");
    }

    #[test]
    fn trims_the_name() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            create(d.path(), "  test \n").unwrap(),
            d.path().join("test.md")
        );
    }

    #[test]
    fn invalid_names_create_nothing() {
        let d = tempfile::tempdir().unwrap();
        for bad in ["", "   ", "a/b", "/abs", ".hidden", " .env", "../x"] {
            assert!(normalize(bad).is_none(), "{bad:?}");
            assert_eq!(
                create(d.path(), bad).unwrap_err().kind(),
                ErrorKind::InvalidInput,
                "{bad:?}"
            );
        }
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    }
}
