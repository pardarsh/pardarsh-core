mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use pardarsh_core::interchange::{self, Completeness, ImportPolicy};
use pardarsh_core::prelude::*;

fn person(name: &str) -> ActorRef {
    ActorRef::System { name: name.into() }
}

#[test]
fn visibility_and_disclosure() {
    let mut repo = memory_repo();
    let reporter = person("reporter");
    let private_src =
        SourceRef::new("https://example.org/home-photo.jpg").visibility(Visibility::restricted(["moderators"]));
    let public_src = SourceRef::new("https://example.org/street.jpg");
    let issue = repo
        .create(
            issue_draft("Open manhole")
                .actor(ActorRoleKind::Reporter, Attribution::withheld(reporter.clone()))
                .source(private_src)
                .source(public_src),
            Meta::by(Attribution::withheld(reporter.clone())),
        )
        .unwrap()
        .record;
    let note: NamespacedName = "test.civic:moderation_note".parse().unwrap();
    repo.annotate(
        &issue.id,
        note,
        BTreeMap::from([("text".into(), ExtValue::Text("reporter is a repeat flagger".into()))]),
        Meta::by(Attribution::system("moderator")).visibility(Visibility::restricted(["moderators"])),
    )
    .unwrap();
    let hidden = repo
        .create(issue_draft("Hidden").visibility(Visibility::restricted(["moderators"])), Meta::by(alice()))
        .unwrap()
        .record;
    let record = repo.get(&issue.id).unwrap();
    let history = repo.history(&issue.id).unwrap();

    let public = Viewer::public();
    let v = view_record(&record, &public).unwrap();
    assert_eq!(v.sources.len(), 1, "restricted source must be stripped");
    assert_eq!(v.actors[0].attribution.actor, ActorRef::Anonymous, "withheld identity must be masked");
    let h = view_history(&history, &public);
    assert_eq!(h.len(), 1, "restricted moderation note must be removed");
    assert_eq!(h[0].attribution.actor, ActorRef::Anonymous);
    let EventKind::Created { record: snap } = &h[0].kind else { panic!() };
    assert_eq!(snap.sources.len(), 1);
    assert!(view_record(&repo.get(&hidden.id).unwrap(), &public).is_none());

    // The reporter sees their own identity; moderators see everything.
    let own = view_record(&record, &Viewer::actor(reporter.clone())).unwrap();
    assert_eq!(own.actors[0].attribution.actor, reporter);
    let moderator = Viewer::actor(person("mod")).with_audience("moderators");
    assert_eq!(view_record(&record, &moderator).unwrap().sources.len(), 2);
    assert_eq!(view_history(&history, &moderator).len(), 2);
    assert!(view_record(&repo.get(&hidden.id).unwrap(), &moderator).is_some());

    // Public exports carry none of the restricted material.
    let bundle = interchange::export(&repo, &[issue.id.clone(), hidden.id.clone()], &public).unwrap();
    assert_eq!(bundle.completeness, Completeness::Filtered);
    assert_eq!(bundle.entries.len(), 1);
    let json = bundle.to_json_pretty().unwrap();
    for secret in ["home-photo", "repeat flagger", "\"name\": \"reporter\"", "Hidden"] {
        assert!(!json.contains(secret), "export leaked {secret}");
    }
}

fn populated() -> (Repository<MemoryStore>, Vec<RecordId>) {
    let mut repo = memory_repo();
    let issue = repo.create(issue_draft("Broken streetlight"), Meta::by(alice())).unwrap().record.id;
    let ev = repo
        .create(
            RecordDraft::new(RecordKind::Evidence, "Photo")
                .source(SourceRef::new("https://example.org/p.jpg").authority(SourceAuthority::UserSubmitted))
                .relation(RelationDraft::new(RelationKind::Supports, issue.clone())),
            Meta::by(alice()),
        )
        .unwrap()
        .record
        .id;
    repo.amend(&issue, vec![Change::Summary(Some("Out for two weeks".into()))], Meta::by(alice())).unwrap();
    (repo, vec![issue, ev])
}

fn importer(origin: &str) -> Repository<MemoryStore> {
    Repository::new(MemoryStore::new(Origin::new(origin).unwrap()), Validator::new(registry())).with_clock(clock())
}

#[test]
fn export_import_round_trip() {
    let (repo, ids) = populated();
    let bundle = interchange::export(&repo, &ids, &Viewer::privileged()).unwrap();
    assert_eq!(bundle.completeness, Completeness::Complete);
    let bytes = serde_json::to_vec(&bundle).unwrap();

    let parsed = interchange::parse_bundle(&bytes, &ImportPolicy::default()).unwrap();
    assert_eq!(parsed, bundle);

    let mut other = importer("mirror.test");
    let report = interchange::import(&mut other, &parsed, &ImportPolicy::default()).unwrap();
    assert_eq!(report.records, 2);
    assert_eq!(report.events_written, 3);
    for id in &ids {
        assert_eq!(other.get(id).unwrap(), repo.get(id).unwrap(), "identity and state preserved");
        assert_eq!(other.history(id).unwrap(), repo.history(id).unwrap(), "history preserved");
    }
    // Re-import is idempotent.
    let again = interchange::import(&mut other, &parsed, &ImportPolicy::default()).unwrap();
    assert_eq!(again.events_written, 0);
    assert_eq!(again.events_already_present, 3);

    // A later export appends only the new events.
    let (mut repo, ids) = (repo, ids);
    repo.amend(&ids[0], vec![Change::Title("Broken streetlight, 4th Cross".into())], Meta::by(alice())).unwrap();
    let newer = interchange::export(&repo, &ids, &Viewer::privileged()).unwrap();
    let r = interchange::import(&mut other, &newer, &ImportPolicy::default()).unwrap();
    assert_eq!(r.events_written, 1);
}

#[test]
fn import_rejects_untrusted_or_tampered_bundles() {
    let (repo, ids) = populated();
    let bundle = interchange::export(&repo, &ids, &Viewer::privileged()).unwrap();
    let policy = ImportPolicy::default();

    assert!(interchange::parse_bundle(b"{not json", &policy).is_err());
    assert!(
        interchange::parse_bundle(&[b' '; 100], &ImportPolicy { max_bytes: 10, ..ImportPolicy::default() }).is_err()
    );

    // Tampered content: the bundle hash no longer matches.
    let mut t = bundle.clone();
    t.entries[0].record.title = "Something else".into();
    assert!(interchange::verify_bundle(&t, &policy).is_err());

    // Re-hashed but inconsistent: the record no longer matches its history.
    t.entries[0].record_hash = interchange::content_hash(&t.entries[0].record).unwrap();
    t.bundle_hash = None;
    t.bundle_hash = Some(interchange::content_hash(&t).unwrap());
    let err = interchange::verify_bundle(&t, &policy).unwrap_err().to_string();
    assert!(err.contains("does not match its history"), "{err}");

    // Unknown format version.
    let mut t = bundle.clone();
    t.format_version = SchemaVersion::new(2, 0);
    assert!(interchange::verify_bundle(&t, &policy).is_err());

    // Records that claim to be ours are refused unless restoring a backup.
    let mut same_origin = importer("local.test");
    assert!(interchange::import(&mut same_origin, &bundle, &policy).is_err());
    interchange::import(&mut same_origin, &bundle, &ImportPolicy { allow_own_origin: true, ..ImportPolicy::default() })
        .unwrap();

    // Origin allow-list.
    let mut other = importer("mirror.test");
    let trusted = ImportPolicy {
        trusted_origins: Some(BTreeSet::from([Origin::new("x.test").unwrap()])),
        ..ImportPolicy::default()
    };
    assert!(interchange::import(&mut other, &bundle, &trusted).is_err());

    // Filtered (public) bundles are for reading, not for import.
    let public = interchange::export(&repo, &ids, &Viewer::public()).unwrap();
    assert!(interchange::import(&mut other, &public, &policy).is_err());

    // Imported records are validated like local ones (unknown extension).
    let mut strict = Repository::new(MemoryStore::new(Origin::new("strict.test").unwrap()), Validator::default());
    assert!(matches!(interchange::import(&mut strict, &bundle, &policy), Err(Error::Validation(_))));

    // Divergent local history is detected, not overwritten.
    let mut diverged = importer("mirror.test");
    interchange::import(&mut diverged, &bundle, &policy).unwrap();
    let mut forked = bundle.clone();
    forked.entries[0].history[1].id = EventId::generate();
    forked.entries[0].history[1].kind = EventKind::Amended { changes: vec![Change::Summary(Some("forked".into()))] };
    let projected = pardarsh_core::projection::project(&forked.entries[0].history).unwrap().unwrap();
    forked.entries[0].record = projected;
    forked.entries[0].record_hash = interchange::content_hash(&forked.entries[0].record).unwrap();
    forked.bundle_hash = None;
    forked.bundle_hash = Some(interchange::content_hash(&forked).unwrap());
    let err = interchange::import(&mut diverged, &forked, &policy).unwrap_err().to_string();
    assert!(err.contains("diverges"), "{err}");
}

#[test]
fn canonical_json_is_key_ordered_and_stable() {
    let v = serde_json::json!({"b": 1, "a": {"d": [3, {"z": 1, "y": 2}], "c": "x"}});
    let bytes = interchange::canonical_json(&v).unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap(), r#"{"a":{"c":"x","d":[3,{"y":2,"z":1}]},"b":1}"#);
    let h = interchange::content_hash(&v).unwrap();
    assert!(h.as_str().starts_with("sha256:"));
    assert_eq!(h, interchange::content_hash(&v).unwrap());
}
