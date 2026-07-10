//! Browser bindings for the ruleset parser.
//!
//! The design spine — "one core, two targets, one contract" — means the JSON this
//! module returns for a dropped PDF is byte-for-byte identical to what the native
//! `build` writes for the same file (both go through `serde_json::to_string_pretty`
//! + `markdown::render`), which is what makes the live-diff feature trustworthy.

use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Install a panic hook so a malformed PDF surfaces as a console error instead of
/// an opaque `unreachable`. Note: unlike the native binary (which isolates
/// `pdf-extract` panics with `catch_unwind`), a panic here traps the module and
/// the page must be reloaded — the UI states this.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// The result of parsing one PDF in the browser: the pretty JSON (identical to the
/// native `build` output), the rendered Markdown, and the document title.
#[derive(Serialize)]
struct Parsed {
    title: String,
    json: String,
    markdown: String,
}

/// Parse a dropped PDF's bytes into `{title, json, markdown}`.
#[wasm_bindgen]
pub fn parse(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let doc = kochi_university_regulations::extract::parse_pdf_bytes(bytes)
        .map_err(|e| JsValue::from_str(&format!("{e:#}")))?;
    let json =
        serde_json::to_string_pretty(&doc).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let markdown = kochi_university_regulations::markdown::render(&doc);
    let out = Parsed {
        title: doc.title,
        json,
        markdown,
    };
    serde_wasm_bindgen::to_value(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}
