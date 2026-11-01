import base64
import unittest

import restore_drill as rd


class RestoreDrillTest(unittest.TestCase):
    def test_pinned_image(self):
        text = "OPENBAO_IMAGE_VERSION=2.7.2\nOPENBAO_IMAGE_DIGEST=sha256:" + "a" * 64 + "\n"
        self.assertEqual(rd.pinned_image(text), "quay.io/openbao/openbao:2.7.2@sha256:" + "a" * 64)
        with self.assertRaises(SystemExit):
            rd.pinned_image("OPENBAO_IMAGE_VERSION=2.7.2\n")

    def test_decode_otp_token(self):
        otp = "abcdefghijklmnopqrstuvwxyz"
        token = "x" * 25  # synthetic, same length class as a real token
        encoded = base64.b64encode(bytes(a ^ b for a, b in zip(token.encode(), otp.encode()))).decode()
        self.assertEqual(rd.decode_otp_token(encoded.rstrip("="), otp), token)

    def test_expected_and_compare(self):
        ess = [
            {"metadata": {"namespace": "ns", "name": "a"},
             "spec": {"dataFrom": [{"extract": {"key": "k8s/a"}}]}},
            {"metadata": {"namespace": "ns", "name": "b-es"},
             "spec": {"target": {"name": "b"},
                      "data": [{"secretKey": "t", "remoteRef": {"key": "k8s/a", "property": "x"}},
                               {"secretKey": "gone", "remoteRef": {"key": "k8s/none", "property": "y"}}]}},
            {"metadata": {"namespace": "ns", "name": "c"},
             "spec": {"dataFrom": [{"extract": {"key": "k8s/missing"}}]}},
        ]
        kv = {"k8s/a": {"x": "1", "y": "2"}}
        expected = rd.expected_secret_hashes(ess, kv)
        self.assertEqual(expected[("ns", "a", "x")], rd.digest("1"))
        self.assertEqual(expected[("ns", "b", "t")], rd.digest("1"))
        self.assertIsNone(expected[("ns", "b", "gone")])
        self.assertIsNone(expected[("ns", "c", "<extract k8s/missing>")])

        live = {("ns", "a", "x"): rd.digest("1"), ("ns", "a", "y"): rd.digest("changed"),
                ("ns", "b", "t"): rd.digest("1")}
        problems = rd.compare(expected, live)
        self.assertIn("value differs: ns/a/y", problems)
        self.assertIn("missing in snapshot: ns/b/gone", problems)
        self.assertIn("missing in snapshot: ns/c/<extract k8s/missing>", problems)
        self.assertNotIn("value differs: ns/a/x", problems)

    def test_templated_checks_presence_only(self):
        ess = [{"metadata": {"namespace": "m", "name": "am"},
                "spec": {"target": {"name": "am", "template": {"data": {"cfg": "{{ .t }}"}}},
                         "data": [{"secretKey": "t", "remoteRef": {"key": "k", "property": "t"}},
                                  {"secretKey": "u", "remoteRef": {"key": "k", "property": "u"}}]}}]
        expected = rd.expected_secret_hashes(ess, {"k": {"t": "x"}})
        self.assertEqual(expected[("m", "am", "t")], rd.TEMPLATED)
        self.assertEqual(rd.compare(expected, {}), ["missing in snapshot: m/am/u"])


if __name__ == "__main__":
    unittest.main()
