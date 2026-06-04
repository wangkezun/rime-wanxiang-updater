use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

pub struct SafeList {
    patterns: Vec<String>,
    set: GlobSet,
}

impl SafeList {
    pub fn new(patterns: &[String]) -> Result<Self> {
        let mut b = GlobSetBuilder::new();
        for p in patterns {
            b.add(Glob::new(p)?);
        }
        Ok(Self { patterns: patterns.to_vec(), set: b.build()? })
    }

    pub fn patterns(&self) -> &[String] { &self.patterns }

    pub fn defaults_plus(extra: &[String]) -> Result<Self> {
        let defaults = [
            "*.custom.yaml",
            "installation.yaml",
            "user.yaml",
            "*.userdb*",
            "*.userdb.txt",
            "sync/**",
            "build/**",
        ];
        let merged: Vec<String> = defaults.iter().map(|s| s.to_string()).chain(extra.iter().cloned()).collect();
        Self::new(&merged)
    }

    pub fn is_protected<P: AsRef<Path>>(&self, rel: P) -> bool {
        self.set.is_match(rel.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_custom_yaml() {
        let s = SafeList::defaults_plus(&[]).unwrap();
        assert!(s.is_protected("wanxiang.custom.yaml"));
        assert!(s.is_protected("user.yaml"));
        assert!(!s.is_protected("wanxiang.schema.yaml"));
    }

    #[test]
    fn protects_userdb_recursive() {
        let s = SafeList::defaults_plus(&[]).unwrap();
        assert!(s.is_protected("pinyin.userdb.txt"));
        assert!(s.is_protected("sync/foo/bar.yaml"));
        assert!(s.is_protected("build/anything"));
    }

    #[test]
    fn extra_patterns_merge() {
        let s = SafeList::defaults_plus(&["mine.yaml".to_string()]).unwrap();
        assert!(s.is_protected("mine.yaml"));
        assert!(s.is_protected("user.yaml"));
    }
}
