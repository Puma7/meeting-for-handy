use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// A uniquely owned synthetic profile that is removed when the test releases it.
pub(crate) struct TestProfile(PathBuf);

impl TestProfile {
    pub(crate) fn new() -> io::Result<Self> {
        let parent = std::env::temp_dir().canonicalize()?;
        let path = parent.join(format!("hm-test-profile-{}", meeting_notes::new_job_id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestProfile {
    fn drop(&mut self) {
        if self
            .0
            .canonicalize()
            .as_ref()
            .is_ok_and(|path| path == &self.0)
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
