//! 路径 6：流式查询 benchmark（3 基准点）
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn stream_cursor(c: &mut Criterion) {
    let data: Vec<i64> = (0..1000).collect();
    c.bench_function("stream_cursor", |b| {
        b.iter(|| {
            let mut sum = 0i64;
            for &v in black_box(&data).iter() {
                sum += v;
            }
            black_box(sum);
        })
    });
}

fn stream_buffered(c: &mut Criterion) {
    let data: Vec<i64> = (0..1000).collect();
    c.bench_function("stream_buffered", |b| {
        b.iter(|| {
            let chunks: Vec<&[i64]> = black_box(&data).chunks(100).collect();
            let mut sum = 0i64;
            for chunk in chunks {
                for &v in chunk {
                    sum += v;
                }
            }
            black_box(sum);
        })
    });
}

fn stream_backpressure(c: &mut Criterion) {
    let data: Vec<i64> = (0..1000).collect();
    c.bench_function("stream_backpressure", |b| {
        b.iter(|| {
            let mut processed = 0;
            for chunk in black_box(&data).chunks(50) {
                if processed >= 500 {
                    break;
                }
                processed += chunk.len();
            }
            black_box(processed);
        })
    });
}

criterion_group!(benches, stream_cursor, stream_buffered, stream_backpressure);
criterion_main!(benches);