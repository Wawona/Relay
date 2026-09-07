use wwn_runtime::{Op::*, Program};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let p = Program::compile(
        &[
            Add {
                dst: 0,
                lhs: 0,
                rhs: 1,
            },
            Mul {
                dst: 0,
                lhs: 0,
                rhs: 2,
            },
            Return { src: 0 },
        ],
        3,
        true,
    )?;
    let result = p.run(&[5, 7, 3], &mut [], 3)?;
    assert_eq!(result.value, 36);
    println!("{{\"engine\":\"wwn-static\",\"result\":{},\"fused_pairs\":{},\"executed_ops\":{},\"dynamic_native_code\":false}}",
        result.value, p.fused_pairs(), result.executed_ops);
    Ok(())
}
