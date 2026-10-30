import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('personal_finetune', Path(__file__).with_name('finetune.py'))
finetune = importlib.util.module_from_spec(spec)
spec.loader.exec_module(finetune)

BASE = {
    'architecture': 'mixednet',
    'pointwise_filters': '64,64,64,64',
    'repeat_in_block': '1,1,1,1',
    'mixconv_kernel_sizes': '[5], [7,11], [9,15], [23]',
    'first_conv_filters': 32,
    'first_conv_kernel_size': 5,
    'stride': 3,
    'spectrogram_length': 204,
}


class ParseTests(unittest.TestCase):
    def test_accepts_comma_separated_positive_widths(self):
        self.assertEqual(finetune.parse_pointwise_filters('128,128,128,128'), (128, 128, 128, 128))
        self.assertEqual(finetune.parse_pointwise_filters(' 96, 96 '), (96, 96))

    def test_rejects_malformed_or_empty_widths(self):
        for value in ['', '128,,128', 'wide', '0,128', '-64,64']:
            with self.assertRaises(ValueError):
                finetune.parse_pointwise_filters(value)


class ArchitectureTests(unittest.TestCase):
    def test_defaults_keep_declared_config(self):
        cfg, architecture = finetune.apply_architecture(dict(BASE), from_scratch=True)
        self.assertEqual(cfg['pointwise_filters'], '64,64,64,64')
        self.assertEqual(cfg['first_conv_filters'], 32)
        self.assertFalse(architecture['changed'])
        self.assertEqual(architecture['stride'], 3)
        self.assertEqual(architecture['spectrogram_length'], 204)

    def test_wider_topology_requires_scratch_initialization(self):
        with self.assertRaises(ValueError):
            finetune.apply_architecture(dict(BASE), (128, 128, 128, 128), 64, from_scratch=False)

    def test_wider_topology_preserves_streaming_contract(self):
        cfg, architecture = finetune.apply_architecture(
            dict(BASE), (128, 128, 128, 128), 64, from_scratch=True)
        self.assertEqual(cfg['pointwise_filters'], '128,128,128,128')
        self.assertEqual(cfg['first_conv_filters'], 64)
        self.assertEqual(cfg['mixconv_kernel_sizes'], BASE['mixconv_kernel_sizes'])
        self.assertEqual(cfg['repeat_in_block'], BASE['repeat_in_block'])
        self.assertEqual(cfg['first_conv_kernel_size'], 5)
        self.assertEqual(cfg['stride'], 3)
        self.assertEqual(cfg['spectrogram_length'], 204)
        self.assertTrue(architecture['changed'])
        self.assertEqual(architecture['mixconv_kernel_sizes'], BASE['mixconv_kernel_sizes'])

    def test_original_config_is_not_mutated(self):
        original = dict(BASE)
        finetune.apply_architecture(original, (128, 128, 128, 128), 64, from_scratch=True)
        self.assertEqual(original, BASE)

    def test_width_count_must_match_residual_blocks(self):
        with self.assertRaises(ValueError):
            finetune.apply_architecture(dict(BASE), (128, 128, 128), 64, from_scratch=True)

    def test_non_positive_first_convolution_rejected(self):
        with self.assertRaises(ValueError):
            finetune.apply_architecture(dict(BASE), None, 0, from_scratch=True)

    def test_unmodified_other_architecture_is_preserved(self):
        inception = {'architecture': 'inception', 'blocks': 4, 'filters': 64}
        cfg, architecture = finetune.apply_architecture(dict(inception))
        self.assertEqual(cfg, inception)
        self.assertEqual(architecture, {'architecture': 'inception', 'changed': False})

    def test_width_override_rejected_for_other_architecture(self):
        inception = {'architecture': 'inception', 'filters': 64}
        with self.assertRaises(ValueError):
            finetune.apply_architecture(dict(inception), (128, 128, 128, 128), 64, from_scratch=True)


class ExportSignatureTests(unittest.TestCase):
    def test_export_signature_converts_numpy_like_scalars_for_json(self):
        import json

        class Scalar:
            def __init__(self, value):
                self.value = value

            def __int__(self):
                return self.value

        details = {'shape': [Scalar(1), Scalar(3), Scalar(40)], 'dtype': 'int8'}
        signature = finetune.export_signature(details)
        self.assertEqual(signature, {'shape': [1, 3, 40], 'dtype': 'int8'})
        self.assertEqual(json.loads(json.dumps(signature)), signature)


@unittest.skipUnless(importlib.util.find_spec('numpy'), 'NumPy required for parameter counting')
class ParameterCountTests(unittest.TestCase):
    def test_uses_count_params_not_file_size(self):
        import numpy as np

        class FakeModel:
            trainable_weights = [np.zeros((10,)), np.zeros((3, 3))]

            def count_params(self):
                return 19

        self.assertEqual(finetune.model_parameter_counts(FakeModel()), {'total': 19, 'trainable': 19})

    def test_numpy_interpreter_metadata_is_json_serializable(self):
        import json
        import numpy as np

        details = {'shape': np.array([1, 3, 40], dtype=np.int32), 'dtype': np.dtype('int8')}
        signature = finetune.export_signature(details)
        self.assertEqual(signature, {'shape': [1, 3, 40], 'dtype': 'int8'})
        self.assertEqual(json.loads(json.dumps(signature)), signature)



if __name__ == '__main__':
    unittest.main()
