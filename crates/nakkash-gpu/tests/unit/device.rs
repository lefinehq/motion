use super::*;
use rstest::rstest;

#[rstest]
#[case::discrete(wgpu::DeviceType::DiscreteGpu, true)]
#[case::integrated(wgpu::DeviceType::IntegratedGpu, true)]
#[case::virtual_gpu(wgpu::DeviceType::VirtualGpu, true)]
#[case::software(wgpu::DeviceType::Cpu, false)]
#[case::unknown(wgpu::DeviceType::Other, false)]
fn only_hardware_adapters_are_used(#[case] kind: wgpu::DeviceType, #[case] used: bool) {
    assert_eq!(is_hardware(kind), used);
}

#[test]
fn a_refusal_names_the_software_adapter() {
    let refusal = Refusal::Software("llvmpipe (LLVM 17.0.6, 256 bits)".into());
    assert!(refusal.to_string().contains("llvmpipe"));
    assert!(refusal.to_string().contains("software"));
}
