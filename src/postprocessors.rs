//! A collection of officially maintained [postprocessors][crate::Postprocessor].

use std::{fs, io::ErrorKind, path::Path};

use pulldown_cmark::{Event, Tag};
use serde_yaml::{Value};
use slug::slugify;

use crate::WriteSnafu;
use snafu::ResultExt;

use super::{Context, MarkdownEvents, PostprocessorResult};

/// This postprocessor converts all soft line breaks to hard line breaks. Enabling this mimics
/// Obsidian's _'Strict line breaks'_ setting.
pub fn softbreaks_to_hardbreaks(
    _context: &mut Context,
    events: &mut MarkdownEvents<'_>,
) -> PostprocessorResult {
    for event in events.iter_mut() {
        if event == &Event::SoftBreak {
            *event = Event::HardBreak;
        }
    }
    PostprocessorResult::Continue
}


pub fn destination_from_frontmatter(
    context: &mut Context,
    events: &mut MarkdownEvents<'_>,
) -> PostprocessorResult {
    let date = context.frontmatter.get("date").and_then(|d| d.as_str()).unwrap_or("1970-01-01").to_owned();
    let title = context.frontmatter.get("title").and_then(|d| d.as_str()).unwrap_or(context.current_file().file_stem().expect("It is a file").to_str().expect("It is a file")).to_owned();
    let slug = slugify(title);
    match context.frontmatter.get("export_to") {
        Some(Value::String(export_path)) => {
            let mut from = context.current_file().as_path();
            let mut to = context.destination.as_path();
            while from.file_name() == to.file_name() {
                to = to.parent().unwrap_or(to);
                from = from.parent().unwrap_or(from);
            }
            let mut target = export_path.replace(":date", &date);
            target = target.replace(":title", &slug);
            context.destination = to.join( Path::new(&target)).to_path_buf();

            for event in events.iter_mut() {
                match event {
                    Event::Start(Tag::Image {
                        link_type: _,
                        dest_url,
                        title: _,
                        id: _,
                    }) => {
                        let d = dest_url.to_string();
                        if !d.starts_with("https://") && !dest_url.to_string().starts_with("/") {
                            let imgsrc = context.current_file().parent().expect("File will have parent.").join(&d);
                            let dest = context.destination.parent().expect("File will have parent.").join(&d);

                            let _ = fs::copy(imgsrc.clone(), dest.clone())
                                .or_else(|err| {
                                    if err.kind() == ErrorKind::NotFound {
                                        let parent = dest.parent().expect("file should have a parent directory");
                                        fs::create_dir_all(parent)?;
                                    }
                                    fs::copy(imgsrc.clone(), dest.clone())
                                })
                                .context(WriteSnafu { path: imgsrc.clone() });
                        }
                    },
                    _ => (),
                }
            }
        },
        _ => {}
    }
    PostprocessorResult::Continue
}

pub fn filter_by_tags(
    skip_tags: Vec<String>,
    only_tags: Vec<String>,
) -> impl Fn(&mut Context, &mut MarkdownEvents<'_>) -> PostprocessorResult {
    move |context: &mut Context, _events: &mut MarkdownEvents<'_>| -> PostprocessorResult {
        match context.frontmatter.get("tags") {
            None => filter_by_tags_(&[], &skip_tags, &only_tags),
            Some(Value::Sequence(tags)) => filter_by_tags_(tags, &skip_tags, &only_tags),
            _ => PostprocessorResult::Continue,
        }
    }
}

pub fn remove_specified_tags(
    to_remove: Vec<String>,
) -> impl Fn(&mut Context, &mut MarkdownEvents<'_>) -> PostprocessorResult {
    move |context: &mut Context, _events: &mut MarkdownEvents<'_>| -> PostprocessorResult {
        let mut result_tags: Vec<String> = vec![];
        match context.frontmatter.get("tags") {
            Some(Value::Sequence(tags)) => {tags.iter().map(|t| {
                let tag = t.as_str().unwrap_or("").to_owned();
                if !to_remove.contains(&tag) {
                    result_tags.push(tag.clone());
                }
            }).for_each(drop); ()},
            _ => (),
        }
        context.frontmatter.insert("tags".into(), result_tags.into());
        return PostprocessorResult::Continue
    }
}

fn filter_by_tags_(
    tags: &[Value],
    skip_tags: &[String],
    only_tags: &[String],
) -> PostprocessorResult {
    let skip = skip_tags
        .iter()
        .any(|tag| tags.contains(&Value::String(tag.to_string())));
    let include = only_tags.is_empty()
        || only_tags
            .iter()
            .any(|tag| tags.contains(&Value::String(tag.to_string())));

    if skip || !include {
        PostprocessorResult::StopAndSkipNote
    } else {
        PostprocessorResult::Continue
    }
}

#[test]
fn test_filter_tags() {
    let tags = vec![
        Value::String("skip".into()),
        Value::String("publish".into()),
    ];
    let empty_tags = vec![];
    assert_eq!(
        filter_by_tags_(&empty_tags, &[], &[]),
        PostprocessorResult::Continue,
        "When no exclusion & inclusion are specified, files without tags are included"
    );
    assert_eq!(
        filter_by_tags_(&tags, &[], &[]),
        PostprocessorResult::Continue,
        "When no exclusion & inclusion are specified, files with tags are included"
    );
    assert_eq!(
        filter_by_tags_(&tags, &["exclude".into()], &[]),
        PostprocessorResult::Continue,
        "When exclusion tags don't match files with tags are included"
    );
    assert_eq!(
        filter_by_tags_(&empty_tags, &["exclude".into()], &[]),
        PostprocessorResult::Continue,
        "When exclusion tags don't match files without tags are included"
    );
    assert_eq!(
        filter_by_tags_(&tags, &[], &["publish".into()]),
        PostprocessorResult::Continue,
        "When exclusion tags don't match files with tags are included"
    );
    assert_eq!(
        filter_by_tags_(&empty_tags, &[], &["include".into()]),
        PostprocessorResult::StopAndSkipNote,
        "When inclusion tags are specified files without tags are excluded"
    );
    assert_eq!(
        filter_by_tags_(&tags, &[], &["include".into()]),
        PostprocessorResult::StopAndSkipNote,
        "When exclusion tags don't match files with tags are exluded"
    );
    assert_eq!(
        filter_by_tags_(&tags, &["skip".into()], &["skip".into()]),
        PostprocessorResult::StopAndSkipNote,
        "When both inclusion and exclusion tags are the same exclusion wins"
    );
    assert_eq!(
        filter_by_tags_(&tags, &["skip".into()], &["publish".into()]),
        PostprocessorResult::StopAndSkipNote,
        "When both inclusion and exclusion tags match exclusion wins"
    );
}
