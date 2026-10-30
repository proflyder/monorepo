#!/usr/bin/env python3
"""Unit tests for the pure helpers in vpnusers.py."""
import base64
import unittest

import vpnusers


class VpnUsersTest(unittest.TestCase):
    def test_base32_secret_is_unpadded_and_reversible(self):
        raw = bytes(range(20))
        encoded = vpnusers.base32_secret(raw)
        self.assertNotIn("=", encoded)
        self.assertEqual(base64.b32decode(encoded + "=" * (-len(encoded) % 8)), raw)

    def test_otpauth_url_has_required_parameters(self):
        url = vpnusers.otpauth_url("stas", "JBSWY3DPEHPK3PXP")
        self.assertTrue(url.startswith("otpauth://totp/"))
        self.assertIn("secret=JBSWY3DPEHPK3PXP", url)
        self.assertIn("issuer=home%20VPN", url)
        self.assertIn("digits=6", url)
        self.assertIn("period=30", url)
        self.assertIn("algorithm=SHA1", url)

    def test_ocpasswd_replaces_user_and_keeps_others(self):
        old = "alice:lan:$6$aaa\nstas:full:$6$old\n"
        new = vpnusers.merge_ocpasswd(old, "stas", "lan,full", "$6$new")
        self.assertEqual(new, "alice:lan:$6$aaa\nstas:lan,full:$6$new\n")

    def test_ocpasswd_appends_new_user(self):
        self.assertEqual(
            vpnusers.merge_ocpasswd("", "stas", "lan,full", "$6$new"),
            "stas:lan,full:$6$new\n",
        )

    def test_otp_replaces_user_and_keeps_comments(self):
        old = "# comment\nHOTP/T30 alice - aa\nHOTP/T30 stas - bb\n"
        new = vpnusers.merge_otp(old, "stas", "cc")
        self.assertEqual(new, "# comment\nHOTP/T30 alice - aa\nHOTP/T30 stas - cc\n")

    def test_otp_does_not_confuse_prefix_usernames(self):
        old = "HOTP/T30 stas2 - aa\n"
        new = vpnusers.merge_otp(old, "stas", "bb")
        self.assertEqual(new, "HOTP/T30 stas2 - aa\nHOTP/T30 stas - bb\n")


if __name__ == "__main__":
    unittest.main()
