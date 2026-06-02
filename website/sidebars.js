const docsSidebar = [
  {
    type: "category",
    label: "Start",
    link: {
      type: "doc",
      id: "overview",
    },
    items: ["overview", "installation", "contributing", "changelog"],
  },
  {
    type: "category",
    label: "Concepts",
    items: ["concepts", "language_design", "workflow_graph_spec"],
  },
  {
    type: "category",
    label: "Implementation",
    items: ["language_implementation", "engineering_architecture", "type_system", "wasm_codegen"],
  },
  {
    type: "category",
    label: "Libraries",
    items: ["stdlib_design", "stdlib_implementation", "http_lib", "file_lib"],
  },
  {
    type: "category",
    label: "Testing",
    items: ["unit_test", "e2e_test", "benchmark_test"],
  },
];

module.exports = {
  docsSidebar,
};
