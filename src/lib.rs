use std::{io::Write, sync::{Arc, RwLock}};
use swc_ecma_parser::{Capturing, JscTarget, Parser, StringInput, Syntax, TsConfig, lexer::Lexer};
use swc_common::{FileName, SourceMap, errors::{ColorConfig, Handler}, sync::Lrc};
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};

use swc_ecma_visit::FoldWith;
use wasm_bindgen::prelude::*;

#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    fn alert(s: &str);
    fn eval(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[derive(Debug, Clone)]
struct Buf(Arc<RwLock<Vec<u8>>>);
impl Write for Buf {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.write().unwrap().write(data)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.write().unwrap().flush()
    }
}

fn eval_ts(filename: &str, contents: &str) -> Result<JsValue, JsValue> {
    let cm: Lrc<SourceMap> = Default::default();
    let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(cm.clone()));

    let source = cm.new_source_file(
        FileName::Custom(filename.to_owned()),
        contents.to_owned(),
    );

    let lexer = Lexer::new(
        Syntax::Typescript(TsConfig {
            dts: filename.ends_with(".d.ts"),
            tsx: filename.contains("tsx"),
            dynamic_import: true,
            decorators: true,
            import_assertions: true,
            no_early_errors: false,
            ..Default::default()
        }),
        JscTarget::Es2016,
        StringInput::from(&*source),
        None,
    );

    let capturing = Capturing::new(lexer);

    let mut parser = Parser::new_from(capturing);

    for e in parser.take_errors() {
        e.into_diagnostic(&handler).emit();
    }

    let module = parser
        .parse_typescript_module()
        .map_err(|e| e.into_diagnostic(&handler).emit())
        .expect("failed to parse module.")
        .fold_with(&mut swc_ecma_transforms_typescript::strip());

    let mut wr = Buf(Arc::new(RwLock::new(vec![])));

    {
        let mut emitter = Emitter {
            cfg: Default::default(),
            cm: cm.clone(),
            wr: Box::new(JsWriter::new(cm, "\n", &mut wr, None)),
            comments: None,
        };
        emitter.emit_module(&module).expect("failed to emit module");
    };

    let code_output = wr.0.read().expect("failed to read code output");
    let output = &*String::from_utf8_lossy(&code_output);
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let elem = document.create_element("script").expect("failed to create script element");
    elem.set_inner_html(output);
    let head = document.head().expect("failed to get head element");
    head.append_child(&elem)?;
    Ok(JsValue::NULL)
}

#[wasm_bindgen]
pub fn main(input: &str) -> Result<JsValue, JsValue> {
    eval_ts("index.ts", input)?;
    Ok(JsValue::NULL)
}

/*
use web_sys::{RequestMode, RequestInit};

#[wasm_bindgen]
pub fn entrypoint(tspath: &str) {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);

    let request = web_sys::Request::new_with_str_and_init(tspath, &opts).expect("failed to create request");

    let cb = Closure::once(move |result: JsValue| {
        if let Some(input) = result.as_string() {
            main(input.as_str());
        } else {
            unsafe { console_log!("unexpected js result {:?}", result) };
        }
    });

    let er = Closure::once(move |_: JsValue| {
        unsafe { console_log!("entrypoint ts file not found") };
    });

    request.text().expect("failed to get text").then(&cb).catch(&er);

    // https://stackoverflow.com/questions/53214434/how-to-return-a-rust-closure-to-javascript-via-webassembly/53219594#53219594
    cb.forget();
    er.forget();
}
*/