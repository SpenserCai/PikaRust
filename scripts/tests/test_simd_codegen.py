"""Fail-closed checks for the portable AVX2 assembly regression gate."""

import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("simd_codegen", Path(__file__).parents[1] / "check-avx2-codegen.py")
codegen = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(codegen)
VECTOR = "\tvpmovsxbw (%rdi), %ymm0\n\tvpaddw %ymm0, %ymm1, %ymm1\n"


class AssemblyTests(unittest.TestCase):
    def test_inlined_vector_kernel_passes(self):
        codegen.check_assembly(VECTOR + "\tvzeroupper\n\tretq\n")

    def test_intrinsic_calls_and_tail_calls_fail(self):
        for instruction in ["call", "callq", "jmp", "jmpq"]:
            for target in ["_ZN4core_mm256_loadu_si25617h123E", "*_ZN4core_mm256_add_epi1617h123E@GOTPCREL(%rip)"]:
                with self.subTest(instruction=instruction, target=target):
                    with self.assertRaisesRegex(ValueError, "Out-of-line"):
                        codegen.check_assembly(VECTOR + f"\t{instruction}\t{target}\n")

    def test_empty_or_scalar_only_assembly_fails(self):
        for assembly in ["", "\taddq %rax, %rcx\n", "\tvmovdqu (%rax), %xmm0\n"]:
            with self.subTest(assembly=assembly):
                with self.assertRaisesRegex(ValueError, "No 256-bit"):
                    codegen.check_assembly(assembly)

    def test_symbol_names_without_calls_are_not_false_positives(self):
        codegen.check_assembly("_ZN4core_mm256_add_epi1617h123E:\n" + VECTOR)


if __name__ == "__main__":
    unittest.main()
