"""Build isolated experimental DLLs under .data; never deploy these variants.

Some variants intentionally omit checks for known-valid benchmark inputs. They
are not production changes. Each build uses a fresh snapshot of local crates.
"""
from pathlib import Path
import argparse
import os
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
VARIANTS = ('inline', 'host_cpu', 'no_view_checks', 'no_index_validation',
            'reused_buffers', 'specialized_modes', 'scalar_inline')
CRATES = ('portfolio-risk-native', 'portfolio-model-core', 'portfolio-compute-control')


def snapshot_sources(destination):
    for name in CRATES:
        source = ROOT / 'packages' / name
        target = destination / 'packages' / name
        target.mkdir(parents=True)
        for filename in ('Cargo.toml', 'Cargo.lock'):
            if (source / filename).is_file():
                shutil.copyfile(source / filename, target / filename)
        # Ablations compile only the library, never test-only fixture consumers.
        for directory in ('src',):
            if (source / directory).is_dir():
                shutil.copytree(source / directory, target / directory)
    return destination / 'packages/portfolio-risk-native'


def replace_exact(path, old, new):
    text = path.read_text(encoding='utf-8')
    if text.count(old) != 1:
        raise ValueError(f'{path}: expected one occurrence of {old!r}')
    path.write_text(text.replace(old, new), encoding='utf-8')


def apply_variant(crate, name):
    if name not in VARIANTS:
        raise ValueError(f'unknown ablation: {name}')
    source = crate / 'src'
    mortgage = source / 'products/mortgages/kernels.rs'
    deposits = source / 'products/deposits/kernels.rs'
    arrays = source / 'math/arrays.rs'
    if name == 'inline':
        replace_exact(mortgage, 'fn mortgage_step(', '#[inline(always)]\nfn mortgage_step(')
    elif name == 'no_view_checks':
        for assertion in (
            '        assert!(p < self.shape[0] && m < self.shape[1] && self.shape[2] == 1);',
            '        assert!(s < self.shape[0] && p < self.shape[1] && h < self.shape[2]);',
        ):
            replace_exact(arrays, assertion, '')
    elif name == 'no_index_validation':
        replace_exact(arrays, '        assert!(x >= 0.0 && x.fract() == 0.0 && x < usize::MAX as f64);', '')
    elif name == 'reused_buffers':
        for path in (mortgage, deposits):
            replace_exact(path, 'let mut buf = vec![0.0; t];', '')
            replace_exact(path, '            for path in 0..p {',
                          '            let mut buf = vec![0.0; t];\n            for path in 0..p {')
    elif name == 'specialized_modes':
        for path, function in ((mortgage, 'mortgage'), (deposits, 'deposits')):
            signature = f"pub(crate) fn {function}(a: &[View<'_>], stress: bool) -> Vec<Vec<f64>> {{"
            wrapper = (signature + f'\n    if stress {{ {function}_mode::<true>(a) }} '
                       + f'else {{ {function}_mode::<false>(a) }}\n}}\n'
                       + f"fn {function}_mode<const STRESS: bool>(a: &[View<'_>]) -> Vec<Vec<f64>> {{\n"
                       + '    let stress = STRESS;')
            replace_exact(path, signature, wrapper)
    elif name == 'scalar_inline':
        for function in ('sigmoid', 'lut', 'spline'):
            replace_exact(source / 'math/functions.rs', 'pub(crate) fn ' + function + '(',
                          '#[inline(always)]\npub(crate) fn ' + function + '(')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('variants', nargs='*', choices=VARIANTS)
    parser.add_argument('--check', action='store_true', help='Type-check isolated variants without release builds')
    args = parser.parse_args()
    for name in args.variants or ('inline', 'host_cpu'):
        target = (ROOT / '.data/rust-product-ablation' / name).resolve()
        assert target.is_relative_to((ROOT / '.data').resolve())
        target.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='source-', dir=target) as temporary:
            # Verify the exact absolute tree that TemporaryDirectory cleans up.
            snapshot = Path(temporary).resolve()
            assert snapshot.is_relative_to(target)
            crate = snapshot_sources(snapshot)
            apply_variant(crate, name)
            env = os.environ.copy()
            cache = target / 'target' if not args.check else target.parent / 'check-target'
            env['CARGO_TARGET_DIR'] = str(cache)
            if name == 'host_cpu':
                env['RUSTFLAGS'] = '-C target-cpu=native'
            command = ['cargo', 'check' if args.check else 'build', '--locked', '--lib']
            if not args.check:
                command.append('--release')
            print(f'{name}: isolated {command[1]}', flush=True)
            subprocess.run(command + ['--manifest-path', str(crate / 'Cargo.toml')], env=env, check=True)


if __name__ == '__main__':
    main()
