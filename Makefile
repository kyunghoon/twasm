build:
	$(BUILDER) wasm-pack build --release --target web
	echo "export const ts_import = async f => main(f, await(await fetch(f)).text());\nexport const ts_entrypoint = f => init().then(() => ts_import(f));" >> ./pkg/twasm.js
