import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('personal_finetune', Path(__file__).with_name('finetune.py'))
finetune = importlib.util.module_from_spec(spec)
spec.loader.exec_module(finetune)

BATCH = 128


def counts(**flags):
    return {(source, key): count for source, key, _, count
            in finetune.mixture_buckets(BATCH, **flags)}


class MixtureBucketTests(unittest.TestCase):
    """Counts for batch=128; they must match the pre-refactor recipes exactly."""

    def test_plain_mixture(self):
        self.assertEqual(counts(hard=False, station=False, synthetic_hard=False, extra=False), {
            ('real', 'train_pos'): 32,
            ('synthetic', 'train_pos'): 32,
            ('real', 'train_neg'): 32,
            ('synthetic', 'train_neg'): 32,
        })

    def test_hard_negatives(self):
        self.assertEqual(counts(hard=True, station=False, synthetic_hard=False, extra=False), {
            ('real', 'train_pos'): 32,
            ('synthetic', 'train_pos'): 32,
            ('real', 'train_neg'): 16,
            ('synthetic', 'train_neg'): 16,
            ('real', 'hard_train_neg'): 32,
        })

    def test_station_only(self):
        self.assertEqual(counts(hard=False, station=True, synthetic_hard=False, extra=False), {
            ('real', 'train_pos'): 32,
            ('synthetic', 'train_pos'): 32,
            ('real', 'train_neg'): 16,
            ('synthetic', 'train_neg'): 16,
            ('real', 'station_train_neg'): 32,
        })

    def test_hard_and_station(self):
        self.assertEqual(counts(hard=True, station=True, synthetic_hard=False, extra=False), {
            ('real', 'train_pos'): 32,
            ('synthetic', 'train_pos'): 32,
            ('real', 'train_neg'): 16,
            ('synthetic', 'train_neg'): 16,
            ('real', 'hard_train_neg'): 16,
            ('real', 'station_train_neg'): 16,
        })

    def test_synthetic_confusables(self):
        self.assertEqual(counts(hard=True, station=False, synthetic_hard=True, extra=False), {
            ('real', 'train_pos'): 32,
            ('synthetic', 'train_pos'): 32,
            ('real', 'train_neg'): 16,
            ('synthetic', 'train_neg'): 16,
            ('real', 'hard_train_neg'): 16,
            ('real', 'hard_synth_train_neg'): 16,
        })

    def test_extra_voice(self):
        self.assertEqual(counts(hard=True, station=False, synthetic_hard=False, extra=True), {
            ('real', 'train_pos'): 16,
            ('synthetic', 'train_pos'): 32,
            ('extra', 'train_pos'): 16,
            ('real', 'train_neg'): 16,
            ('real', 'hard_train_neg'): 16,
            ('synthetic', 'train_neg'): 16,
            ('extra', 'train_neg'): 16,
        })

    def test_extra_voice_with_explicit_hard_flag_is_unchanged(self):
        self.assertEqual(
            counts(hard=True, station=False, synthetic_hard=False, extra=True),
            counts(hard=False, station=False, synthetic_hard=False, extra=True))

    def test_extra_voice_and_station(self):
        self.assertEqual(counts(hard=True, station=True, synthetic_hard=False, extra=True), {
            ('real', 'train_pos'): 16,
            ('synthetic', 'train_pos'): 32,
            ('extra', 'train_pos'): 16,
            ('real', 'train_neg'): 16,
            ('real', 'hard_train_neg'): 8,
            ('real', 'station_train_neg'): 8,
            ('synthetic', 'train_neg'): 16,
            ('extra', 'train_neg'): 16,
        })

    def test_extra_voice_and_station_without_hard(self):
        self.assertEqual(counts(hard=False, station=True, synthetic_hard=False, extra=True), {
            ('real', 'train_pos'): 16,
            ('synthetic', 'train_pos'): 32,
            ('extra', 'train_pos'): 16,
            ('real', 'train_neg'): 16,
            ('real', 'station_train_neg'): 16,
            ('synthetic', 'train_neg'): 16,
            ('extra', 'train_neg'): 16,
        })

    def test_extra_voice_and_station_split_the_hard_slot(self):
        with_hard = counts(hard=True, station=True, synthetic_hard=False, extra=True)
        without_hard = counts(hard=False, station=True, synthetic_hard=False, extra=True)
        self.assertEqual(with_hard[('real', 'hard_train_neg')], 8)
        self.assertEqual(with_hard[('real', 'station_train_neg')], 8)
        self.assertNotIn(('real', 'hard_train_neg'), without_hard)
        self.assertEqual(without_hard[('real', 'station_train_neg')], 16)


class MixtureInvariantTests(unittest.TestCase):
    COMBINATIONS = [
        dict(hard=False, station=False, synthetic_hard=False, extra=False),
        dict(hard=True, station=False, synthetic_hard=False, extra=False),
        dict(hard=False, station=True, synthetic_hard=False, extra=False),
        dict(hard=True, station=True, synthetic_hard=False, extra=False),
        dict(hard=True, station=False, synthetic_hard=True, extra=False),
        dict(hard=True, station=False, synthetic_hard=False, extra=True),
        dict(hard=False, station=False, synthetic_hard=False, extra=True),
        dict(hard=True, station=True, synthetic_hard=False, extra=True),
        dict(hard=False, station=True, synthetic_hard=False, extra=True),
    ]

    def test_counts_fill_the_batch_and_keys_are_unique(self):
        for flags in self.COMBINATIONS:
            with self.subTest(**flags):
                buckets = finetune.mixture_buckets(BATCH, **flags)
                keys = [(source, key) for source, key, _, _ in buckets]
                self.assertEqual(len(keys), len(set(keys)))
                self.assertEqual(sum(count for *_, count in buckets), BATCH)
                for source, key, label, count in buckets:
                    self.assertIn(source, {'real', 'synthetic', 'extra'})
                    self.assertIn(label, {0, 1})
                    self.assertGreater(count, 0)
                    self.assertEqual(label, 1 if key == 'train_pos' else 0)

    def test_batch_must_divide_evenly(self):
        for flags in self.COMBINATIONS:
            with self.subTest(**flags):
                with self.assertRaises(ValueError):
                    finetune.mixture_buckets(BATCH + 3, **flags)

    def test_station_and_extra_require_sixteen_way_split(self):
        with self.assertRaises(ValueError):
            finetune.mixture_buckets(120, hard=True, station=True,
                                     synthetic_hard=False, extra=True)


class MixtureRecipeTests(unittest.TestCase):
    def test_station_and_extra_recipe_keeps_v27_shares(self):
        self.assertEqual(
            finetune.mixture_recipe(hard=True, station=True, synthetic_hard=False, extra=True),
            "12.5% personal-positive / 25% original-positive / "
            "12.5% extra-voice-positive / 12.5% personal-negative / "
            "6.25% real-hard-negative / 6.25% station-false-positive / "
            "12.5% original-negative / 12.5% extra-voice-negative")

    def test_station_and_extra_recipe_without_hard(self):
        self.assertEqual(
            finetune.mixture_recipe(hard=False, station=True, synthetic_hard=False, extra=True),
            "12.5% personal-positive / 25% original-positive / "
            "12.5% extra-voice-positive / 12.5% personal-negative / "
            "12.5% station-false-positive / 12.5% original-negative / "
            "12.5% extra-voice-negative")

    def test_existing_extra_recipe_is_unchanged(self):
        self.assertEqual(
            finetune.mixture_recipe(hard=True, station=False, synthetic_hard=False, extra=True),
            "12.5% personal-positive / 25% original-positive / "
            "12.5% extra-voice-positive / 12.5% personal-negative / "
            "12.5% real-hard-negative / 12.5% original-negative / "
            "12.5% extra-voice-negative")

    def test_shares_match_bucket_counts(self):
        shares = finetune.mixture_shares(128, hard=True, station=True,
                                         synthetic_hard=False, extra=True)
        self.assertEqual(shares['station_negative'], 8 / 128)
        self.assertEqual(shares['hard_negative'], 8 / 128)
        self.assertEqual(sum(shares.values()), 1)


if __name__ == '__main__':
    unittest.main()
