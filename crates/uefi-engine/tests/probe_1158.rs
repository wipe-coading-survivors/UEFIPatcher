use std::time::Instant;

const PATH: &str = "../../../refs/amibcp/226D2IL3.30";

#[test]
fn probe_1158() {
    let data = std::fs::read(PATH).unwrap();
    let t0 = Instant::now();
    let img = uefi_engine::parser::image::parse_image(
        &data,
        uefi_engine::types::ImageMode::Read,
        "probe",
        "s",
    )
    .unwrap();
    println!("parse: {:?}", t0.elapsed());

    let t1 = Instant::now();
    let forms = uefi_engine::hii::forms::collect_forms(&img);
    println!("collect_forms ({}) : {:?}", forms.len(), t1.elapsed());

    let t2 = Instant::now();
    let _qs =
        uefi_engine::hii::list_questions(&img, "899407D7-99FE-43D8-9A21-79EC328CAC21:0x18:0", 1158)
            .unwrap();
    println!("list_questions(1158) first: {:?}", t2.elapsed());
    let t3 = Instant::now();
    let qs =
        uefi_engine::hii::list_questions(&img, "899407D7-99FE-43D8-9A21-79EC328CAC21:0x18:0", 1158)
            .unwrap();
    println!("list_questions(1158) second: {:?}", t3.elapsed());
    println!(
        "=== form 1158 questions: {} (subsets of prompt/qid/offset/seed)",
        qs.len()
    );
    for q in qs.iter().take(30) {
        println!(
            "  qid={:#06x} kind={} prompt={:?} off={} w={} seed={:?} default={:?}",
            q.question_id, q.kind, q.prompt, q.var_offset, q.width, q.seed_value, q.ifr_default
        );
    }
    let t4 = Instant::now();
    let edges = uefi_engine::hii::ref_tree::collect_edges(&img);
    println!("collect_edges ({}) : {:?}", edges.len(), t4.elapsed());

    let qs1025 =
        uefi_engine::hii::list_questions(&img, "899407D7-99FE-43D8-9A21-79EC328CAC21:0x18:0", 1025)
            .unwrap();
    println!("=== form 1025 questions: {}", qs1025.len());
    for q in qs1025.iter().take(10) {
        println!(
            "  qid={:#06x} kind={} prompt={:?} off={} w={} seed={:?}",
            q.question_id, q.kind, q.prompt, q.var_offset, q.width, q.seed_value
        );
    }
}
