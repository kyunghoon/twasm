build:
	$(BUILDER) wasm-pack build --release --target web
	@echo "window.ts_import = async f => fetch(f).then(r => r.text()).then(d => main(f, d));\nexport const ts_entrypoint = f => init().then(() => ts_import(f));" >> ./pkg/twasm.js

example:
	cargo run --example example1
