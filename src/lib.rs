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

use std::{io::Write, path::PathBuf, sync::{Arc, RwLock}};
use swc_ecma_parser::{Capturing, JscTarget, Parser, StringInput, Syntax, TsConfig, lexer::Lexer};
use swc_common::{FileName, SourceMap, errors::{ColorConfig, Handler}, sync::Lrc};
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};

use swc_ecma_visit::FoldWith;
use wasm_bindgen::prelude::*;

#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    fn alert(s: &str);
    fn eval(s: &str);
}

macro_rules! console_log { ($($t:tt)*) => (#[allow(unused_unsafe)] unsafe { log(&format_args!($($t)*).to_string()) }) }

#[derive(Debug)]
enum Error {
    JSError(JsValue),
    ECMAParseError(swc_ecma_parser::error::Error),
    IOError(std::io::Error),
    PoisonError(String),
    DiagnosticEmitted,
    InvalidWindow,
    InvalidDocument,
    InvalidHead,
}
impl From<JsValue> for Error { fn from(e: JsValue) -> Error { Error::JSError(e) } }
impl From<std::io::Error> for Error { fn from(e: std::io::Error) -> Error { Error::IOError(e) } }
impl From<swc_ecma_parser::error::Error> for Error { fn from(e: swc_ecma_parser::error::Error) -> Error { Error::ECMAParseError(e) } }
impl<T> From<std::sync::PoisonError<T>> for Error { fn from(e: std::sync::PoisonError<T>) -> Error { Error::PoisonError(e.to_string()) } }

type Result<T> = std::result::Result<T, Error>;

mod keyid {
    use std::sync::atomic::{AtomicU64, Ordering};
    static KEYID: AtomicU64 = AtomicU64::new(0);
    pub fn new() -> u64 { KEYID.fetch_add(1, Ordering::SeqCst) }
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

fn transpile(filename: &str, input: &str) -> Result<u64> {
    swc_common::GLOBALS.set(&swc_common::Globals::new(), || {
        let cm: Lrc<SourceMap> = Default::default();
        let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(cm.clone()));

        let source = cm.new_source_file(
            FileName::Real(PathBuf::from(filename)),
            input.to_owned(),
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
            .map_err(|e| { e.into_diagnostic(&handler).emit(); Error::DiagnosticEmitted })?
            .fold_with(&mut swc_ecma_transforms_typescript::strip())
            .fold_with(&mut swc_ecma_transforms_module::amd::amd(Default::default()));
            //.fold_with(&mut swc_ecma_transforms_module::umd::umd(cm.clone(), Mark::fresh(Mark::root()), Default::default()));

        let mut wr = Buf(Arc::new(RwLock::new(vec![])));

        {
            let mut emitter = Emitter {
                cfg: Default::default(),
                cm: cm.clone(),
                wr: Box::new(JsWriter::new(cm, "\n", &mut wr, None)),
                comments: None,
            };
            emitter.emit_module(&module)?;
        };

        let code_output = wr.0.read()?;
        let output = &*String::from_utf8_lossy(&code_output);

        let keyid = keyid::new();

        let window = web_sys::window().ok_or(Error::InvalidWindow)?;
        let document = window.document().ok_or(Error::InvalidDocument)?;
        let head = document.head().ok_or(Error::InvalidHead)?;
        let elem = document.create_element("script")?;
        elem.set_inner_html(format!("define({}, {}", keyid, &output[7..]).as_str());
        head.append_child(&elem)?;

        Ok(keyid)
    })
}

#[wasm_bindgen]
pub fn main(filename: &str, input: &str) -> std::result::Result<JsValue, JsValue> {
    match transpile(filename, input) {
        Err(e) => Err(JsValue::from_str(format!("{:?}", e).as_str())),
        Ok(keyid) => Ok(JsValue::from_f64(keyid as f64)),
    }
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
