import unittest

from preflight import LEGACY_WRITERS, legacy_writer_failures


class LegacyWriterTests(unittest.TestCase):
    def test_absent_is_safe(self):
        self.assertEqual(legacy_writer_failures({}), [])

    def test_off_is_safe(self):
        states = {name: "off" for name in LEGACY_WRITERS}
        self.assertEqual(legacy_writer_failures(states), [])

    def test_mixed_absent_and_off_is_safe(self):
        states = {LEGACY_WRITERS[0]: "off", LEGACY_WRITERS[2]: "off"}
        self.assertEqual(legacy_writer_failures(states), [])

    def test_on_fails(self):
        states = {name: "off" for name in LEGACY_WRITERS}
        states[LEGACY_WRITERS[0]] = "on"
        failures = legacy_writer_failures(states)
        self.assertEqual(len(failures), 1)
        self.assertIn(LEGACY_WRITERS[0], failures[0])

    def test_unknown_and_unavailable_fail(self):
        for bad in ("unknown", "unavailable"):
            states = {LEGACY_WRITERS[1]: bad}
            failures = legacy_writer_failures(states)
            self.assertEqual(len(failures), 1, bad)
            self.assertIn(LEGACY_WRITERS[1], failures[0])

    def test_unexpected_state_fails(self):
        failures = legacy_writer_failures({LEGACY_WRITERS[3]: "idle"})
        self.assertEqual(len(failures), 1)


if __name__ == "__main__":
    unittest.main()
