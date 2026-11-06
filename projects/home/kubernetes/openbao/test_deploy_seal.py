"""The deploy must never replace the live static seal key with a new one."""
import pathlib
import re
import unittest

DEPLOY = (pathlib.Path(__file__).resolve().parent / "deploy.sh").read_text()


class SealKeyGuard(unittest.TestCase):
    def test_new_key_only_when_no_live_secret(self):
        block = re.search(r'if \[\[ ! -s "\$SECRETS_DIR/seal.key" && "\$seal_secret_exists" == 0 \]\]; then\n'
                          r'  openssl rand', DEPLOY)
        self.assertIsNotNone(block)
        self.assertEqual(DEPLOY.count("openssl rand"), 1)

    def test_live_secret_is_checked_before_generation(self):
        self.assertLess(DEPLOY.index("get secret openbao-seal"), DEPLOY.index("openssl rand"))

    def test_secret_applied_only_from_an_existing_local_key(self):
        apply = DEPLOY.index("create secret generic openbao-seal")
        guard = DEPLOY.rindex('if [[ -s "$SECRETS_DIR/seal.key" ]]; then', 0, apply)
        self.assertLess(guard, apply)


if __name__ == "__main__":
    unittest.main()
