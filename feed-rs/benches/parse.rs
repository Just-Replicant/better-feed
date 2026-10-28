//! Parser benchmarks: `cargo bench -p better-feed` (add `-- <filter>` to run a subset).

use std::hint::black_box;
use std::ops::ControlFlow;

use better_feed::parser::{self, Builder};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

// A synthetic RSS 2.0 podcast feed with iTunes, MediaRSS, Podcasting 2.0 and unknown extension elements per item
fn synthetic_rss(items: usize) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd" xmlns:media="http://search.yahoo.com/mrss/"
     xmlns:podcast="https://podcastindex.org/namespace/1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:x="urn:example">
<channel><title>Synthetic</title><link>https://example.com/</link><description>Benchmark feed</description>
<itunes:author>Bench</itunes:author><podcast:guid>bench</podcast:guid>
"#,
    );
    for i in 0..items {
        xml.push_str(&format!(
            r#"<item><title>Episode {i} &amp; more</title><link>https://example.com/{i}</link><guid>id-{i}</guid>
<pubDate>Tue, 15 Nov 2022 20:15:04 +0000</pubDate><dc:creator>Author {i}</dc:creator>
<description><![CDATA[<p>Some <b>HTML</b> description for item {i}</p>]]></description>
<enclosure url="https://example.com/{i}.mp3" type="audio/mpeg" length="12345"/>
<itunes:duration>1:02:03</itunes:duration><itunes:episode>{i}</itunes:episode><itunes:explicit>no</itunes:explicit>
<media:content url="https://example.com/{i}.mp4" type="video/mp4"><media:title>Video {i}</media:title></media:content>
<podcast:chapters url="https://example.com/{i}.json" type="application/json+chapters"/>
<x:custom a="1"><x:nested>value {i}</x:nested></x:custom></item>
"#
        ));
    }
    xml.push_str("</channel></rss>");
    xml
}

fn bench_synthetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("synthetic_rss");
    for items in [100, 2_000] {
        let xml = synthetic_rss(items);
        group.throughput(Throughput::Bytes(xml.len() as u64));

        let default = Builder::new().sanitize_content(false).build();
        group.bench_with_input(BenchmarkId::new("parse", items), &xml, |b, xml| {
            b.iter(|| default.parse(black_box(xml.as_bytes())).unwrap())
        });

        let no_capture = Builder::new().sanitize_content(false).capture_extensions(false).build();
        group.bench_with_input(BenchmarkId::new("parse_no_extensions", items), &xml, |b, xml| {
            b.iter(|| no_capture.parse(black_box(xml.as_bytes())).unwrap())
        });

        group.bench_with_input(BenchmarkId::new("parse_with_stream", items), &xml, |b, xml| {
            b.iter(|| {
                let mut n = 0usize;
                default
                    .parse_with(black_box(xml.as_bytes()), |e| {
                        n += black_box(e).id.len();
                        ControlFlow::Continue(())
                    })
                    .unwrap();
                n
            })
        });

        let sanitising = Builder::new().build();
        group.bench_with_input(BenchmarkId::new("parse_default_builder", items), &xml, |b, xml| {
            b.iter(|| sanitising.parse(black_box(xml.as_bytes())).unwrap())
        });
    }
    group.finish();
}

// Real-world fixtures from the repository (skipped when run outside a checkout, e.g. from the published crate)
fn bench_fixtures(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixtures");
    for path in [
        "fixture/rss2/rss_2.0_nightvale.xml",
        "fixture/atom/atom_spec_1.xml",
        "fixture/jsonfeed/jsonfeed_elastic_1.1.json",
        "fixture/rss1/rss_1.0_spec_1.xml",
    ] {
        let Ok(data) = std::fs::read(path) else { continue };
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(path.rsplit('/').next().unwrap()), &data, |b, data| {
            b.iter(|| parser::parse(black_box(data.as_slice())).unwrap())
        });
    }
    group.finish();
}

criterion_group!(benches, bench_synthetic, bench_fixtures);
criterion_main!(benches);
