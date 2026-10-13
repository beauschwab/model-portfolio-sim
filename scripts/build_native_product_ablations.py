"""Build isolated experimental DLLs under .data; never deploy these variants.
Some variants intentionally omit redundant checks to measure their cost on known
valid benchmark inputs. They are not proposed production changes.
"""
from pathlib import Path
import os,subprocess,sys
root=Path(__file__).resolve().parents[1]
source=root/'packages/portfolio-risk-native'
for name in (sys.argv[1:] or ('inline','host_cpu')):
    assert name in {'inline','host_cpu','no_view_checks','no_index_validation','reused_buffers','specialized_modes','scalar_inline'}
    target=root/'.data/rust-product-ablation'/name
    (target/'src').mkdir(parents=True,exist_ok=True)
    for namefile in ('Cargo.toml','Cargo.lock','src/lib.rs','src/quant.rs'):
        (target/namefile).write_bytes((source/namefile).read_bytes())
    if name=='inline':
        path=target/'src/quant.rs';text=path.read_text(encoding='utf-8')
        assert text.count('fn mortgage_step(')==1
        path.write_text(text.replace('fn mortgage_step(', '#[inline(always)]\nfn mortgage_step('),encoding='utf-8')
    if name in ('no_view_checks','no_index_validation','reused_buffers'):
        path=target/'src/quant.rs';text=path.read_text(encoding='utf-8')
        if name=='no_view_checks':
            text=text.replace('        assert!(p < self.shape[0] && m < self.shape[1] && self.shape[2] == 1);','')
            text=text.replace('        assert!(s < self.shape[0] && p < self.shape[1] && h < self.shape[2]);','')
        elif name=='no_index_validation':
            text=text.replace('        assert!(x >= 0.0 && x.fract() == 0.0 && x < usize::MAX as f64);','')
        else:
            assert text.count('let mut buf = vec![0.0; t];')==2
            text=text.replace('let mut buf = vec![0.0; t];','')
            text=text.replace('            for path in 0..p {','            let mut buf = vec![0.0; t];\n            for path in 0..p {')
        path.write_text(text,encoding='utf-8')
    if name=='specialized_modes':
        path=target/'src/quant.rs';text=path.read_text(encoding='utf-8')
        for fn in ('mortgage','deposits'):
            text=text.replace(f"fn {fn}(a: &[View<'_>], stress: bool) -> Vec<Vec<f64>> {{",
                f"fn {fn}<const STRESS: bool>(a: &[View<'_>]) -> Vec<Vec<f64>> {{\n    let stress = STRESS;")
            for value in ('true','false'):
                text=text.replace(f'{fn}(&views, {value})',f'{fn}::<{value}>(&views)')
        path.write_text(text,encoding='utf-8')
    if name=='scalar_inline':
        path=target/'src/quant.rs';text=path.read_text(encoding='utf-8')
        for fn in ('sigmoid','lut','spline'):
            text=text.replace(f'fn {fn}(',f'#[inline(always)]\nfn {fn}(')
        path.write_text(text,encoding='utf-8')
    env=os.environ.copy()
    if name=='host_cpu':env['RUSTFLAGS']='-C target-cpu=native'
    subprocess.run(['cargo','build','--release','--locked','--manifest-path',str(target/'Cargo.toml')],env=env,check=True)
