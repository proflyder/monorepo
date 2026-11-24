import pathlib
import stat
import tempfile
import unittest

import device_enroll


class Code(unittest.TestCase):
    def test_known_base32_vector(self):
        secret = device_enroll.base32_encode(b"12345678901234567890")
        expected = "".join(
            ("GEZD", "GNBV", "GY3T", "QOJQ", "GEZD", "GNBV", "GY3T", "QOJQ"))
        self.assertEqual(secret, expected)

    def test_generated_codes_are_base32_and_unique(self):
        code = device_enroll.generate_code()
        self.assertEqual(len(code), 26)
        self.assertTrue(set(code) <= set(device_enroll.ALPHABET))
        self.assertNotEqual(code, device_enroll.generate_code())

    def test_codes_are_normalized_before_hashing(self):
        self.assertEqual(device_enroll.normalize_code(" abcd-efgh\n"), "ABCDEFGH")
        self.assertEqual(
            device_enroll.hash_code("abcd efgh"),
            device_enroll.hash_code("ABCDEFGH"),
        )
        self.assertEqual(len(device_enroll.hash_code("ABCDEFGH")), 64)


class Plan(unittest.TestCase):
    def test_a_new_code_keeps_unrelated_fields_and_drops_the_device(self):
        values = {
            "unrelated": "keep",
            "device_id": "old",
            "device_public_key": "old",
            "device_fingerprint": "old",
        }
        updated, code = device_enroll.plan(values, 1000, revoke=False)
        self.assertIsNotNone(code)
        self.assertEqual(updated["unrelated"], "keep")
        self.assertNotIn("device_revoke_device_id", updated)
        self.assertEqual(updated["enrollment_expires_at"], "1900")
        self.assertEqual(updated["enrollment_code_hash"],
                         device_enroll.hash_code(code))
        self.assertNotIn("device_id", updated)
        self.assertNotIn("device_public_key", updated)

    def test_a_second_code_replaces_the_previous_one(self):
        first, _ = device_enroll.plan({}, 1000, revoke=False)
        second, code = device_enroll.plan(first, 1010, revoke=False)
        self.assertNotEqual(first["enrollment_code_hash"],
                            second["enrollment_code_hash"])
        self.assertEqual(second["enrollment_expires_at"], "1910")
        self.assertEqual(second["enrollment_code_hash"],
                         device_enroll.hash_code(code))

    def test_revoke_clears_the_device_and_leaves_a_revocation_marker(self):
        values = {
            "unrelated": "keep",
            "device_id": "old",
            "device_public_key": "old",
            "device_fingerprint": "old",
            "enrollment_code_hash": "hash",
            "enrollment_expires_at": "1900",
        }
        updated, code = device_enroll.plan(values, 2000, revoke=True)
        self.assertIsNone(code)
        self.assertEqual(updated, {
            "unrelated": "keep",
            "device_revoke_device_id": "old",
            "device_revoked_at": "2000",
        })

    def test_revoke_without_a_device_leaves_no_marker(self):
        updated, code = device_enroll.plan({}, 2000, revoke=True)
        self.assertIsNone(code)
        self.assertEqual(updated, {})


class Files(unittest.TestCase):
    def test_the_code_file_is_private(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "nested" / "device.txt"
            device_enroll.write_private(path, "CODE\n")
            self.assertEqual(path.read_text(), "CODE\n")
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            self.assertEqual(stat.S_IMODE(path.parent.stat().st_mode), 0o700)


if __name__ == "__main__":
    unittest.main()
