//! Browser bindings for the ruleset search engine.
//!
//! A thin wasm-bindgen wrapper over `core::search`: the prebuilt `search.idx` is
//! loaded once into an `Engine` and then queried in the browser. The heavier
//! parsing pipeline stays native (the CLI `build`); only the query side ships to
//! wasm, so `pdf-extract`/`lopdf` never enter this module.

use kochi_university_regulations::search::SearchIndex;
use wasm_bindgen::prelude::*;

/// Install a panic hook so an internal error surfaces as a console message
/// instead of an opaque `unreachable`.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Folding full-text search over the prebuilt `search.idx`. Loaded once in the
/// worker, then queried repeatedly — the heavy index parse stays off the main
/// thread. `query` returns `Vec<Hit>` (`{code, article, title, snippet, hl_start,
/// hl_len, score}`) as a plain JS array; the frontend maps `code`→display name.
#[wasm_bindgen]
pub struct Engine {
    index: SearchIndex,
}

#[wasm_bindgen]
impl Engine {
    /// Build an engine from the raw `search.idx` bytes.
    #[wasm_bindgen(js_name = fromIndex)]
    pub fn from_index(bytes: &[u8]) -> Result<Engine, JsValue> {
        SearchIndex::from_bytes(bytes)
            .map(|index| Engine { index })
            .map_err(|e| JsValue::from_str(&format!("{e:#}")))
    }

    /// Run a query, returning up to `limit` ranked `Hit`s as a JS array.
    pub fn query(&self, q: &str, limit: usize) -> Result<JsValue, JsValue> {
        let hits = self.index.query(q, limit);
        serde_wasm_bindgen::to_value(&hits).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Number of documents in the index (for a "searching N rulesets" hint).
    #[wasm_bindgen(js_name = docCount)]
    pub fn doc_count(&self) -> usize {
        self.index.doc_count()
    }
}
