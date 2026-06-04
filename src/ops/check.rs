use crate::config::Config;
use crate::github::Github;
use crate::manifest::Manifest;
use crate::resource::registry;
use anyhow::Result;
use futures_util::future::join_all;
use serde::Serialize;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    NotInstalled,
    UpToDate,
    HasUpdate,
    Error,
}

#[derive(Debug, Serialize, Clone)]
pub struct ResourceCheck {
    pub id: String,
    pub local_tag: Option<String>,
    pub remote_tag: Option<String>,
    pub status: Status,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct CheckReport {
    pub resources: Vec<ResourceCheck>,
}

impl CheckReport {
    pub fn any_update(&self) -> bool {
        self.resources.iter().any(|r| r.status == Status::HasUpdate)
    }
    pub fn any_error(&self) -> bool {
        self.resources.iter().any(|r| r.status == Status::Error)
    }
}

pub async fn run(cfg: &Config, gh: &Github, manifest: &Manifest) -> Result<CheckReport> {
    let resources = registry();
    let futs = resources.into_iter().map(|res| async move {
        let id = res.id().to_string();
        let local_tag = manifest.resources.get(&id).map(|e| e.tag.clone());
        match res.latest_remote(gh, cfg).await {
            Ok(rr) => {
                let status = match &local_tag {
                    None => Status::NotInstalled,
                    Some(t) if t == &rr.tag => Status::UpToDate,
                    Some(_) => Status::HasUpdate,
                };
                let local_for_report = match status {
                    Status::NotInstalled => Some("-".to_string()),
                    _ => local_tag.clone(),
                };
                ResourceCheck { id, local_tag: local_for_report, remote_tag: Some(rr.tag), status, error: None }
            }
            Err(e) => ResourceCheck { id, local_tag, remote_tag: None, status: Status::Error, error: Some(e.to_string()) },
        }
    });
    let resources = join_all(futs).await;
    Ok(CheckReport { resources })
}

/// Stable text rendering used by the CLI when --json is not set.
pub fn render_text(rep: &CheckReport) -> String {
    let mut s = String::new();
    s.push_str("resource | local | remote | status\n");
    s.push_str("---------+-------+--------+--------\n");
    for r in &rep.resources {
        s.push_str(&format!(
            "{:<8} | {:<5} | {:<6} | {:?}\n",
            r.id,
            r.local_tag.as_deref().unwrap_or("-"),
            r.remote_tag.as_deref().unwrap_or("-"),
            r.status
        ));
        if let Some(e) = &r.error { s.push_str(&format!("  error: {e}\n")); }
    }
    s
}
