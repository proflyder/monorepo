import assert from "node:assert/strict";
import test from "node:test";
import { findComments, languageFor, stripComments } from "./no_comments.mjs";

const texts = (file, source) => findComments(file, source).map((comment) => comment.text);

test("C-family line, block and doc comments are found", () => {
  assert.deepEqual(texts("a.rs", "//! crate\n/// doc\nfn f() {} // tail\n/* a /* nested */ b */\n"), [
    "//! crate",
    "/// doc",
    "// tail",
    "/* a /* nested */ b */",
  ]);
  assert.deepEqual(texts("A.kt", "/** KDoc */\nval x = 1\n"), ["/** KDoc */"]);
});

test("hash and slashes inside strings, URLs and raw strings are code", () => {
  assert.deepEqual(texts("a.rs", 'let u = "https://x/#y";\nlet r = r#"a // b "# c"#;\nlet c = \'#\';\nfn f<\'a>(x: &\'a str) {}\n'), []);
  assert.deepEqual(texts("a.rs", 'let b = br##"/* "# */"##;\n'), []);
  assert.deepEqual(texts("A.kt", 'val s = "${map["k"] ?: "//"} # x"\nval r = """\n// raw ${a}\n"""\n'), []);
  assert.deepEqual(texts("a.mjs", 'const u = `http://a/${b ? "//" : "/*"}`;\nconst r = /\\/\\/[#]/g;\nconst d = a / b / c;\n'), []);
  assert.deepEqual(texts("a.mjs", "const r = /[/]/; // real\n"), ["// real"]);
});

test("python comments and docstrings, not strings in expressions", () => {
  assert.deepEqual(texts("a.py", '"""Module."""\nx = "#not"  # yes\ndef f():\n    """Doc."""\n    return f"{x}#"\ny = ("a"\n     "b")\n'), [
    '"""Module."""',
    "# yes",
    '"""Doc."""',
  ]);
});

test("shell: $#, ${x#y}, quotes, heredocs and URLs are code", () => {
  const source = '#!/bin/sh\n# real\necho $# "${x#pre}" \'#q\' http://a/#f\ncat <<EOF\n# data\nEOF\necho a#b # tail\n';
  assert.deepEqual(texts("a.sh", source), ["# real", "# tail"]);
});

test("yaml: quoted hashes, block scalars, embedded run scripts", () => {
  const source = 'a: "x # y"\nb: \'it\'\'s # z\'\nc: v#w\nd: it\'s # c1\nrun: |\n  echo "#" # c2\n  # c3\ndata: |\n  # literal\n';
  assert.deepEqual(texts("w.yml", source), ["# c1", "# c2", "# c3"]);
});

test("toml, xml, sql and allowlisted directives", () => {
  assert.deepEqual(texts("a.toml", 'a = "#x" # c\nb = \'#\'\n'), ["# c"]);
  assert.deepEqual(texts("a.xml", '<a b="<!-- no -->"><!-- yes --><![CDATA[<!-- no -->]]></a>\n'), ["<!-- yes -->"]);
  assert.deepEqual(texts("a.sql", "select '--' -- c\n"), ["-- c"]);
  assert.deepEqual(texts("platform/mise-tasks/x.sh", "#!/usr/bin/env bash\n#MISE description=\"x\"\n# shellcheck disable=SC2086\n"), []);
  assert.deepEqual(texts("a.rs", "// SAFETY: valid pointer.\nunsafe { f() }\n"), []);
});

test("languages: vendored code is excluded, shebang decides extensionless files", () => {
  assert.equal(languageFor("x/third_party/a.c"), null);
  assert.equal(languageFor("x/testdata/a.py"), null);
  assert.equal(languageFor("bin/tool", "#!/usr/bin/env node\n"), "js");
  assert.equal(languageFor("BUILD.bazel"), "python");
});

test("stripping removes whole comment lines and trailing comments", () => {
  const source = "fn f() {\n    // a\n\n    x(); // b\n}\n";
  assert.equal(stripComments(source, findComments("a.rs", source)), "fn f() {\n    x();\n}\n");
});
