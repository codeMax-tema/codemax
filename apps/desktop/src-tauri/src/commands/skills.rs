use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::core::error::AppResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntryView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSourceView {
    pub id: String,
    pub path: Option<String>,
    pub exists: bool,
    pub skill_count: usize,
    pub status: String,
    pub entries: Vec<SkillEntryView>,
}

#[tauri::command]
pub fn get_skill_sources(project_path: Option<String>) -> AppResult<Vec<SkillSourceView>> {
    let project_dir = project_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let workspace_dir = project_dir
        .as_ref()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let global_root = user_home_dir().map(|path| path.join(".codex").join("skills"));
    let built_in_root = global_root.as_ref().map(|path| path.join(".system"));

    Ok(build_skill_sources(
        project_dir.map(|path| path.join(".codemax").join("skills")),
        workspace_dir.map(|path| path.join(".codemax").join("skills")),
        global_root,
        built_in_root,
    ))
}

fn build_skill_sources(
    project_root: Option<PathBuf>,
    workspace_root: Option<PathBuf>,
    global_root: Option<PathBuf>,
    built_in_root: Option<PathBuf>,
) -> Vec<SkillSourceView> {
    vec![
        materialize_source("project", project_root),
        materialize_source("workspace", workspace_root),
        materialize_source("global", global_root),
        materialize_source("builtIn", built_in_root),
    ]
}

fn materialize_source(id: &str, root: Option<PathBuf>) -> SkillSourceView {
    let Some(root) = root else {
        return SkillSourceView {
            id: id.to_string(),
            path: None,
            exists: false,
            skill_count: 0,
            status: "unavailable".to_string(),
            entries: Vec::new(),
        };
    };

    let canonical = root.canonicalize().unwrap_or(root);
    let exists = canonical.is_dir();
    let entries = if exists {
        collect_skill_entries(&canonical).unwrap_or_default()
    } else {
        Vec::new()
    };
    let skill_count = entries.len();

    SkillSourceView {
        id: id.to_string(),
        path: Some(canonical.to_string_lossy().to_string()),
        exists,
        skill_count,
        status: if exists {
            "ready".to_string()
        } else {
            "missing".to_string()
        },
        entries,
    }
}

fn collect_skill_entries(root: &Path) -> std::io::Result<Vec<SkillEntryView>> {
    let mut entries = Vec::new();

    if root.join("SKILL.md").is_file() {
        entries.push(read_skill_entry(root)?);
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && path.join("SKILL.md").is_file() {
            entries.push(read_skill_entry(&path)?);
        }
    }

    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

fn read_skill_entry(path: &Path) -> std::io::Result<SkillEntryView> {
    let skill_file = if path.is_dir() {
        path.join("SKILL.md")
    } else {
        path.to_path_buf()
    };
    let content = fs::read_to_string(&skill_file)?;
    let (name, description) = parse_skill_frontmatter(&content).unwrap_or_else(|| {
        (
            path.file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "skill".to_string()),
            String::new(),
        )
    });

    Ok(SkillEntryView {
        id: name.to_lowercase().replace(' ', "-"),
        name,
        description,
        path: skill_file.to_string_lossy().to_string(),
    })
}

fn parse_skill_frontmatter(content: &str) -> Option<(String, String)> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut name = None;
    let mut description = None;
    for line in lines {
        let line = line.trim();
        if line == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(unquote_frontmatter(value));
        }
        if let Some(value) = line.strip_prefix("description:") {
            description = Some(unquote_frontmatter(value));
        }
    }

    name.map(|name| (name, description.unwrap_or_default()))
}

fn unquote_frontmatter(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn user_home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codemax-skills-{label}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn builds_real_skill_source_counts_for_existing_paths() {
        let workspace = temp_path("workspace");
        let project = workspace.join("project");
        let global = temp_path("global");
        fs::create_dir_all(project.join(".codemax/skills/project-skill"))
            .expect("create project skill");
        fs::create_dir_all(workspace.join(".codemax/skills/workspace-skill"))
            .expect("create workspace skill");
        fs::create_dir_all(global.join("global-skill")).expect("create global skill");
        fs::create_dir_all(global.join(".system/system-skill")).expect("create system skill");
        fs::write(
            project.join(".codemax/skills/project-skill/SKILL.md"),
            "---\nname: project-skill\ndescription: Project skill\n---\n",
        )
        .expect("write project skill");
        fs::write(
            workspace.join(".codemax/skills/workspace-skill/SKILL.md"),
            "---\nname: workspace-skill\ndescription: Workspace skill\n---\n",
        )
        .expect("write workspace skill");
        fs::write(
            global.join("global-skill/SKILL.md"),
            "---\nname: global-skill\ndescription: Global skill\n---\n",
        )
        .expect("write global skill");
        fs::write(
            global.join(".system/system-skill/SKILL.md"),
            "---\nname: built-in-skill\ndescription: Built in skill\n---\n",
        )
        .expect("write system skill");

        let sources = build_skill_sources(
            Some(project.join(".codemax/skills")),
            Some(workspace.join(".codemax/skills")),
            Some(global.clone()),
            Some(global.join(".system")),
        );

        assert_eq!(sources[0].status, "ready");
        assert_eq!(sources[0].skill_count, 1);
        assert_eq!(sources[0].entries[0].name, "project-skill");
        assert_eq!(sources[1].status, "ready");
        assert_eq!(sources[1].skill_count, 1);
        assert_eq!(sources[1].entries[0].description, "Workspace skill");
        assert_eq!(sources[2].status, "ready");
        assert_eq!(sources[2].skill_count, 1);
        assert_eq!(sources[2].entries[0].name, "global-skill");
        assert_eq!(sources[3].status, "ready");
        assert_eq!(sources[3].skill_count, 1);
        assert_eq!(sources[3].entries[0].name, "built-in-skill");

        fs::remove_dir_all(workspace).expect("clean workspace");
        fs::remove_dir_all(global).expect("clean global");
    }

    #[test]
    fn marks_project_and_workspace_unavailable_without_selected_project() {
        let sources = build_skill_sources(None, None, None, None);

        assert_eq!(sources[0].status, "unavailable");
        assert_eq!(sources[1].status, "unavailable");
        assert_eq!(sources[2].status, "unavailable");
        assert_eq!(sources[3].status, "unavailable");
    }

    #[test]
    fn parses_frontmatter_name_and_description() {
        let parsed = parse_skill_frontmatter(
            "---\nname: \"memory-cockpit\"\ndescription: 'Manage memory.'\n---\n# Title\n",
        )
        .expect("parse frontmatter");

        assert_eq!(parsed.0, "memory-cockpit");
        assert_eq!(parsed.1, "Manage memory.");
    }
}
