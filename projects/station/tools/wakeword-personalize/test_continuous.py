import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('personal_finetune', Path(__file__).with_name('finetune.py'))
finetune = importlib.util.module_from_spec(spec)
spec.loader.exec_module(finetune)

@unittest.skipUnless(importlib.util.find_spec('numpy'), 'NumPy required for feature extraction tests')
class ContinuousTests(unittest.TestCase):
    def test_whole_track_context_and_unaugmented_validation(self):
        import numpy as np
        seen = []
        def extractor(audio):
            seen.append(len(audio))
            return np.arange(998 * 40).reshape(998, 40)
        requests = [dict(end=5., partition='train'), dict(end=9., partition='val')]
        examples = list(finetune.continuous_examples(np.zeros(160000, dtype=np.int16),
            requests, extractor, np.random.default_rng(42), variants=3))
        self.assertEqual(sum(r['partition']=='train' for r, _ in examples), 3)
        self.assertEqual(sum(r['partition']=='val' for r, _ in examples), 1)
        self.assertEqual(seen[0], 160000)
        self.assertTrue(all(length > 150000 for length in seen))
        validation = next(x for r, x in examples if r['partition']=='val')
        np.testing.assert_array_equal(validation, np.arange(998 * 40).reshape(998, 40)[694:898])

if __name__ == '__main__':
    unittest.main()
