use std::{io::Write, path::PathBuf, sync::{Arc, RwLock}};
use swc_ecma_parser::{Capturing, JscTarget, Parser, StringInput, Syntax, TsConfig, lexer::Lexer};
use swc_common::{FileName, Mark, SourceMap, errors::{ColorConfig, Handler}, sync::Lrc};
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};
use swc_ecma_visit::FoldWith;

#[derive(Debug)]
enum Error {
    ECMAParseError(swc_ecma_parser::error::Error),
    IOError(std::io::Error),
    PoisonError(String),
    DiagnosticEmitted,
}
impl From<std::io::Error> for Error { fn from(e: std::io::Error) -> Error { Error::IOError(e) } }
impl From<swc_ecma_parser::error::Error> for Error { fn from(e: swc_ecma_parser::error::Error) -> Error { Error::ECMAParseError(e) } }
impl<T> From<std::sync::PoisonError<T>> for Error { fn from(e: std::sync::PoisonError<T>) -> Error { Error::PoisonError(e.to_string()) } }

type Result<T> = std::result::Result<T, Error>;

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

fn transpile(filename: &str, input: &str) -> Result<String> {
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
            .fold_with(&mut swc_ecma_transforms_module::umd::umd(cm.clone(), Mark::fresh(Mark::root()), Default::default()));

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
        let output = String::from_utf8_lossy(&code_output).to_string();

        Ok(output)
    })
}

fn main() {
    let input = "let x = (y: string) => console.log('hello world');";
    match transpile("index.ts", input) {
        Err(e) => println!("{:?}", e),
        Ok(output) => {
            println!("{}", output);
        }
    }
}

/*
(function(global, factory) {
    if (typeof define === "function" && define.amd) {
        define([
            "./test"
        ], factory);
    } else if (typeof exports !== "undefined") {
        factory(require("./test"));
    } else {
        var mod = {
            exports: {
            }
        };
        factory(global.test);
        global.index = mod.exports;
    }
})(this, function(_test) {
    "use strict";
    alert((0, _test).test('a'));
});

>===========>

(function(global, factory) {
    ts_import('./test.ts').then(() => factory(test)).catch(console.error);
})(this, function(_test) {
    "use strict";
    alert((0, _test).test('a'));
});
*/