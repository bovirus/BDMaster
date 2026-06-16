/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::bdrom::udf::{UdfFile, UdfImage};

pub(crate) fn read_disc_title_iso(img: &mut UdfImage) -> Option<String> {
  let meta_dir = img.try_resolve("BDMV/META")?;
  if !meta_dir.is_directory {
    return None;
  }
  fn walk_for_bdmt_eng(img: &mut UdfImage, dir: &UdfFile) -> Option<Vec<u8>> {
    let entries = img.list_dir(dir).ok()?;
    for e in entries {
      if e.is_parent || e.is_deleted {
        continue;
      }
      let child = crate::bdrom::udf::read_file_entry_at(img, &e.icb).ok()?;
      if child.is_directory {
        if let Some(bytes) = walk_for_bdmt_eng(img, &child) {
          return Some(bytes);
        }
      } else if e.name.eq_ignore_ascii_case("bdmt_eng.xml") {
        return img.read_file(&child).ok();
      }
    }
    None
  }
  let bytes = walk_for_bdmt_eng(img, &meta_dir)?;
  let text = String::from_utf8_lossy(&bytes).to_string();
  extract_title_from_xml(&text)
}

pub(crate) fn read_disc_title_native(meta_dir: &Path) -> Option<String> {
  fn walk(dir: &Path, visited: &mut HashSet<PathBuf>, depth: usize, out: &mut Option<PathBuf>) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH || out.is_some() {
      return;
    }

    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(key) {
      return;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
      for entry in entries.flatten() {
        if out.is_some() {
          return;
        }
        let Ok(file_type) = entry.file_type() else {
          continue;
        };
        if file_type.is_symlink() {
          continue;
        }
        let p = entry.path();
        if file_type.is_dir() {
          walk(&p, visited, depth + 1, out);
        } else if file_type.is_file()
          && p
            .file_name()
            .map(|n| n.to_string_lossy().eq_ignore_ascii_case("bdmt_eng.xml"))
            .unwrap_or(false)
        {
          *out = Some(p);
        }
      }
    }
  }
  let mut visited = HashSet::new();
  let mut found = None;
  walk(meta_dir, &mut visited, 0, &mut found);
  let path = found.as_ref()?;
  let text = std::fs::read_to_string(path).ok()?;
  extract_title_from_xml(&text)
}

pub(crate) fn extract_title_from_xml(xml: &str) -> Option<String> {
  let mut pos = 0usize;
  let mut stack: Vec<String> = Vec::new();

  while let Some(start_rel) = xml[pos..].find('<') {
    let start = pos + start_rel;
    let Some(end_rel) = xml[start..].find('>') else {
      return None;
    };
    let end = start + end_rel;
    let raw = xml[start + 1..end].trim();
    pos = end + 1;

    if raw.starts_with('?') || raw.starts_with('!') {
      continue;
    }

    if let Some(close) = raw.strip_prefix('/') {
      let close_name = xml_local_name(close);
      while let Some(open_name) = stack.pop() {
        if open_name == close_name {
          break;
        }
      }
      continue;
    }

    let self_closing = raw.ends_with('/');
    let tag_name = xml_local_name(raw.trim_end_matches('/').trim());
    let parent = stack.last().map(String::as_str);

    if tag_name == "name" && parent == Some("title") {
      let content_start = end + 1;
      let Some(close_start_rel) = xml[content_start..].find("</") else {
        return None;
      };
      let close_start = content_start + close_start_rel;
      let Some(close_end_rel) = xml[close_start..].find('>') else {
        return None;
      };
      let close_end = close_start + close_end_rel;
      if xml_local_name(xml[close_start + 2..close_end].trim()) == "name" {
        let title = xml_decode_text(xml[content_start..close_start].trim());
        if !title.is_empty() && !title.eq_ignore_ascii_case("blu-ray") {
          return Some(title);
        }
      }
    }

    if !self_closing {
      stack.push(tag_name);
    }
  }
  None
}

fn xml_local_name(raw: &str) -> String {
  let name = raw
    .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
    .next()
    .unwrap_or_default();
  name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

fn xml_decode_text(text: &str) -> String {
  if !text.contains('&') {
    return text.to_string();
  }

  let mut out = String::new();
  let mut rest = text;
  while let Some(amp) = rest.find('&') {
    out.push_str(&rest[..amp]);
    let after_amp = &rest[amp + 1..];
    let Some(semi) = after_amp.find(';') else {
      out.push('&');
      rest = after_amp;
      continue;
    };
    let entity = &after_amp[..semi];
    match entity {
      "amp" => out.push('&'),
      "lt" => out.push('<'),
      "gt" => out.push('>'),
      "quot" => out.push('"'),
      "apos" => out.push('\''),
      _ if entity.starts_with("#x") => {
        if let Ok(code) = u32::from_str_radix(&entity[2..], 16) {
          if let Some(ch) = char::from_u32(code) {
            out.push(ch);
          }
        }
      }
      _ if entity.starts_with('#') => {
        if let Ok(code) = entity[1..].parse::<u32>() {
          if let Some(ch) = char::from_u32(code) {
            out.push(ch);
          }
        }
      }
      _ => {
        out.push('&');
        out.push_str(entity);
        out.push(';');
      }
    }
    rest = &after_amp[semi + 1..];
  }
  out.push_str(rest);
  out
}
