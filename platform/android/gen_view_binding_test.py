#!/usr/bin/env python3
"""Tests for gen_view_binding.py. Run: python3 -I platform/android/gen_view_binding_test.py"""

import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
GENERATOR = os.path.join(HERE, "gen_view_binding.py")

LAYOUT = """<?xml version="1.0" encoding="utf-8"?>
<LinearLayout xmlns:android="http://schemas.android.com/apk/res/android"
    android:layout_width="match_parent" android:layout_height="match_parent">
    <TextView android:id="@+id/item_title" />
    <View android:id="@id/divider" />
    <com.google.android.material.button.MaterialButton android:id="@+id/item_save" />
    <TextView android:id="@+id/item_title" />
    <TextView android:id="@+id/root" />
    <include layout="@layout/other" android:id="@+id/included" />
    <TextView android:text="no id" />
</LinearLayout>
"""


def generate(layouts):
    with tempfile.TemporaryDirectory() as tmp:
        paths = []
        for name, body in layouts.items():
            path = os.path.join(tmp, name)
            with open(path, "w", encoding="utf-8") as f:
                f.write(body)
            paths.append(path)
        out = os.path.join(tmp, "out.kt")
        result = subprocess.run(
            [sys.executable, "-I", GENERATOR, "--package", "a.b", "--output", out, *paths],
            capture_output=True,
            text=True,
        )
        text = ""
        if os.path.exists(out):
            with open(out, encoding="utf-8") as f:
                text = f.read()
        return result, text


class GenViewBindingTest(unittest.TestCase):
    def test_generates_typed_properties(self):
        result, kt = generate({"fragment_sample.xml": LAYOUT})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("package a.b.databinding", kt)
        self.assertIn("import a.b.R", kt)
        self.assertIn("class FragmentSampleBinding private constructor(", kt)
        self.assertIn("override val root: android.widget.LinearLayout,", kt)
        self.assertIn("val itemTitle: android.widget.TextView,", kt)
        self.assertIn("val divider: android.view.View,", kt)
        self.assertIn("val itemSave: com.google.android.material.button.MaterialButton,", kt)
        self.assertIn("val included: android.view.View,", kt)
        self.assertIn("R.layout.fragment_sample", kt)

    def test_duplicate_ids_and_root_name_clash(self):
        _, kt = generate({"fragment_sample.xml": LAYOUT})
        self.assertEqual(kt.count("val itemTitle:"), 1)
        self.assertIn("val rootView: android.widget.TextView,", kt)

    def test_one_class_per_layout_sorted(self):
        _, kt = generate({"b_two.xml": LAYOUT, "a_one.xml": LAYOUT})
        self.assertLess(kt.index("class AOneBinding"), kt.index("class BTwoBinding"))

    def test_merge_root_is_rejected(self):
        result, _ = generate({"x.xml": '<merge xmlns:android="http://schemas.android.com/apk/res/android"/>'})
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
