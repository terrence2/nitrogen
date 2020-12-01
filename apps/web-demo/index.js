const nitrogen = import('./pkg');

nitrogen
  .then(m => m.wasm_main())
  .catch(console.error);
