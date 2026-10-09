mod common;

use std::collections::BTreeMap;

use common::*;
use pardarsh_core::graph::{self, Direction, TraversalOptions};
use pardarsh_core::prelude::*;

fn actor<S: Store>(repo: &mut Repository<S>, name: &str) -> RecordId {
    repo.create(RecordDraft::new(RecordKind::Actor, name), Meta::by(alice())).unwrap().record.id
}

fn invalid(err: Error) -> Vec<String> {
    match err {
        Error::Validation(v) => v.issues.into_iter().map(|i| format!("{}: {}", i.path, i.message)).collect(),
        other => panic!("expected validation error, got {other:?}"),
    }
}

fn create_get_and_history<S: Store>(repo: &mut Repository<S>) {
    let c = repo.create(issue_draft("Pothole on Ring Road").summary("Deep pothole"), Meta::by(alice())).unwrap();
    assert!(!c.replayed);
    assert_eq!(c.record.version, 1);
    assert_eq!(c.event.sequence, 1);
    assert!(c.record.id.is_from(&origin()));
    let got = repo.get(&c.record.id).unwrap();
    assert_eq!(got, c.record);

    repo.amend(&got.id, vec![Change::Title("Pothole near Ring Road junction".into())], Meta::by(alice())).unwrap();
    let r = repo.get(&got.id).unwrap();
    assert_eq!(r.version, 2);
    assert_eq!(r.title, "Pothole near Ring Road junction");
    let history = repo.history(&got.id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(pardarsh_core::projection::project(&history).unwrap().unwrap(), r);

    let tl = repo.timeline(&got.id).unwrap();
    assert_eq!(tl[1].event, "amended");
    assert_eq!(tl[1].changes[0].field, "title");
    assert_eq!(tl[1].changes[0].before.as_deref(), Some("Pothole on Ring Road"));
}

fn rejects_malformed_records<S: Store>(repo: &mut Repository<S>) {
    let e = repo.create(issue_draft("   "), Meta::by(alice())).unwrap_err();
    assert!(invalid(e).iter().any(|m| m.starts_with("title")));

    let e = repo.create(issue_draft("x").source(SourceRef::new("javascript:alert(1)")), Meta::by(alice())).unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("sources[0].locator")));

    let e = repo.create(issue_draft("bad\u{0007}title"), Meta::by(alice())).unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("control")));

    let e = repo
        .create(
            RecordDraft::new(RecordKind::Issue, "x").extension("unknown.ns", SchemaVersion::new(1, 0), BTreeMap::new()),
            Meta::by(alice()),
        )
        .unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("unknown extension namespace")));

    // Required field missing, wrong enum, wrong type, out of range, newer minor.
    let bad = BTreeMap::from([
        ("locality".to_string(), ExtValue::Integer(3)),
        ("severity".to_string(), ExtValue::Integer(9)),
    ]);
    let e = repo
        .create(
            RecordDraft::new(RecordKind::Issue, "x").extension(NS, SchemaVersion::new(1, 1), bad),
            Meta::by(alice()),
        )
        .unwrap_err();
    let msgs = invalid(e);
    assert!(msgs.iter().any(|m| m.contains("category: required")));
    assert!(msgs.iter().any(|m| m.contains("locality: value does not match")));
    assert!(msgs.iter().any(|m| m.contains("severity: integer out of range")));
    assert!(msgs.iter().any(|m| m.contains("schema_version")));

    // Extension attached to a kind it does not apply to.
    let e = repo
        .create(
            RecordDraft::new(RecordKind::Decision, "x").extension(NS, SchemaVersion::new(1, 0), ext("roads")),
            Meta::by(alice()),
        )
        .unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("does not apply")));

    let e = repo
        .create(
            RecordDraft::new(RecordKind::Issue, "x").extension(
                NS,
                SchemaVersion::new(1, 0),
                BTreeMap::from([("category".into(), ExtValue::Enum("parks".into()))]),
            ),
            Meta::by(alice()),
        )
        .unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("not one of")));

    // Nothing was written by any rejected command.
    assert!(repo.list(&RecordQuery::default()).unwrap().is_empty());
}

fn extension_references_are_checked<S: Store>(repo: &mut Repository<S>) {
    let other_issue = repo.create(issue_draft("other"), Meta::by(alice())).unwrap().record.id;
    let mut fields = ext("water");
    fields.insert("owner".into(), ExtValue::Reference(other_issue));
    let e = repo
        .create(
            RecordDraft::new(RecordKind::Issue, "x").extension(NS, SchemaVersion::new(1, 0), fields),
            Meta::by(alice()),
        )
        .unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("referenced record has kind issue")));

    let dept = actor(repo, "Water Department");
    let mut fields = ext("water");
    fields.insert("owner".into(), ExtValue::Reference(dept));
    repo.create(
        RecordDraft::new(RecordKind::Issue, "x").extension(NS, SchemaVersion::new(1, 0), fields),
        Meta::by(alice()),
    )
    .unwrap();
}

fn optimistic_concurrency<S: Store>(repo: &mut Repository<S>) {
    let id = repo.create(issue_draft("a"), Meta::by(alice())).unwrap().record.id;
    repo.amend(&id, vec![Change::Summary(Some("one".into()))], Meta::by(alice()).expect_version(1)).unwrap();
    let e =
        repo.amend(&id, vec![Change::Summary(Some("two".into()))], Meta::by(alice()).expect_version(1)).unwrap_err();
    assert!(matches!(e, Error::Conflict { expected: 1, actual: 2, .. }), "{e:?}");
    assert_eq!(repo.get(&id).unwrap().summary.as_deref(), Some("one"));
}

fn idempotency<S: Store>(repo: &mut Repository<S>) {
    let first = repo.create(issue_draft("a"), Meta::by(alice()).idempotency_key("create-1")).unwrap();
    let again = repo.create(issue_draft("a"), Meta::by(alice()).idempotency_key("create-1")).unwrap();
    assert!(again.replayed);
    assert_eq!(first.record.id, again.record.id);
    assert_eq!(repo.list(&RecordQuery::default()).unwrap().len(), 1);

    let id = first.record.id;
    let src = || Change::AddSource(SourceRef::new("https://example.org/a"));
    repo.amend(&id, vec![src()], Meta::by(alice()).idempotency_key("add-src")).unwrap();
    // A retry generates a new source id but is still the same command.
    let r = repo.amend(&id, vec![src()], Meta::by(alice()).idempotency_key("add-src")).unwrap();
    assert!(r.replayed);
    assert_eq!(repo.get(&id).unwrap().sources.len(), 1);

    let e =
        repo.amend(&id, vec![Change::Title("other".into())], Meta::by(alice()).idempotency_key("add-src")).unwrap_err();
    assert!(matches!(e, Error::IdempotencyConflict(_)));
    let e = repo
        .amend(&id, vec![Change::Title("x".into())], Meta::by(alice()).idempotency_key("bad key with spaces"))
        .unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("idempotency_key")));
}

fn relations_and_graph<S: Store>(repo: &mut Repository<S>) {
    let issue = repo.create(issue_draft("Streetlight out"), Meta::by(alice())).unwrap().record.id;
    let dup = repo.create(issue_draft("Light not working"), Meta::by(alice())).unwrap().record.id;
    let dept = actor(repo, "Electrical Division");
    let evidence = repo.create(RecordDraft::new(RecordKind::Evidence, "Photo"), Meta::by(alice())).unwrap().record.id;

    // Self links, wrong target kinds, dangling targets, unsourced "sourced" links.
    let e =
        repo.link(&issue, RelationDraft::new(RelationKind::RelatedTo, issue.clone()), Meta::by(alice())).unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("itself")));
    let e = repo
        .link(&issue, RelationDraft::new(RelationKind::AssignedTo, evidence.clone()), Meta::by(alice()))
        .unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("requires a target of kind")));
    let e =
        repo.link(&dup, RelationDraft::new(RelationKind::Duplicates, evidence.clone()), Meta::by(alice())).unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("same kind")));
    let ghost = RecordId::generate(&origin());
    let e = repo.link(&issue, RelationDraft::new(RelationKind::Cites, ghost), Meta::by(alice())).unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("does not exist")));
    let e = repo
        .link(
            &issue,
            RelationDraft::new(RelationKind::AssignedTo, dept.clone()).certainty(Certainty::Sourced),
            Meta::by(alice()),
        )
        .unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("cite at least one source")));

    // Remote targets are accepted and reported as unresolved by traversal.
    let remote = RecordId::generate(&Origin::new("budget.example.gov").unwrap());
    repo.link(
        &issue,
        RelationDraft::new(RelationKind::FundedBy, remote.clone()).certainty(Certainty::Unverified),
        Meta::by(alice()),
    )
    .unwrap();

    let assigned = repo
        .link(
            &issue,
            RelationDraft::new(RelationKind::AssignedTo, dept.clone()).certainty(Certainty::Unverified),
            Meta::by(alice()),
        )
        .unwrap();
    let e =
        repo.link(&issue, RelationDraft::new(RelationKind::AssignedTo, dept.clone()), Meta::by(alice())).unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("identical active relation")));
    repo.link(&dup, RelationDraft::new(RelationKind::Duplicates, issue.clone()), Meta::by(alice())).unwrap();
    repo.link(&evidence, RelationDraft::new(RelationKind::Supports, issue.clone()), Meta::by(alice())).unwrap();

    let incoming = graph::incoming(repo.store(), &issue, &[], &[]).unwrap();
    assert_eq!(incoming.len(), 2);
    let supports = graph::incoming(repo.store(), &issue, &[RelationKind::Supports], &[RecordKind::Evidence]).unwrap();
    assert_eq!(supports[0].source, evidence);

    let t = graph::traverse(repo.store(), &dup, &TraversalOptions { max_depth: 3, ..Default::default() }).unwrap();
    let ids: Vec<_> = t.nodes.iter().map(|(id, _)| id.clone()).collect();
    assert!(ids.contains(&issue) && ids.contains(&dept) && ids.contains(&evidence));
    assert_eq!(t.unresolved, vec![remote]);
    let depth1 = graph::traverse(
        repo.store(),
        &dup,
        &TraversalOptions { max_depth: 1, direction: Direction::Outgoing, ..Default::default() },
    )
    .unwrap();
    assert_eq!(depth1.nodes.len(), 2);

    // Retracting keeps the relation in history but removes it from the graph.
    let rel_id = assigned.record.relations.iter().find(|r| r.kind == RelationKind::AssignedTo).unwrap().id;
    repo.unlink(&issue, rel_id, Meta::by(alice())).unwrap();
    assert!(graph::outgoing(repo.store(), &issue, &[RelationKind::AssignedTo]).unwrap().is_empty());
    assert!(repo.get(&issue).unwrap().relations.iter().any(|r| r.id == rel_id && r.retracted));
    // Re-linking after retraction is allowed.
    repo.link(&issue, RelationDraft::new(RelationKind::AssignedTo, dept), Meta::by(alice())).unwrap();
    assert!(repo.unlink(&issue, rel_id, Meta::by(alice())).is_err());
}

fn corrections_retractions_supersession<S: Store>(repo: &mut Repository<S>) {
    let id = repo.create(issue_draft("Water supply cut"), Meta::by(alice())).unwrap().record.id;
    let first_event = repo.history(&id).unwrap()[0].id;

    let e = repo
        .correct(&id, vec![Change::Title("Water supply cut since Monday".into())], Some(first_event), Meta::by(alice()))
        .unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("reason")));
    let e = repo
        .correct(&id, vec![Change::Title("t".into())], Some(EventId::generate()), Meta::by(alice()).reason("typo"))
        .unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("corrects")));
    repo.correct(
        &id,
        vec![Change::Title("Water supply cut since Monday".into())],
        Some(first_event),
        Meta::by(alice()).reason("wrong day"),
    )
    .unwrap();

    let newer = repo.create(issue_draft("Water supply cut (ward 4)"), Meta::by(alice())).unwrap().record.id;
    let evidence = repo.create(RecordDraft::new(RecordKind::Evidence, "x"), Meta::by(alice())).unwrap().record.id;
    assert!(repo.supersede(&id, &evidence, Meta::by(alice()).reason("x")).is_err());
    assert!(repo.supersede(&id, &newer, Meta::by(alice())).is_err());
    repo.supersede(&id, &newer, Meta::by(alice()).reason("merged into ward-level issue")).unwrap();
    assert_eq!(repo.get(&id).unwrap().lifecycle, Lifecycle::Superseded { by: newer.clone() });

    assert!(repo.retract(&newer, Meta::by(alice())).is_err());
    repo.retract(&newer, Meta::by(alice()).reason("filed in error")).unwrap();
    let e = repo.amend(&newer, vec![Change::Title("x".into())], Meta::by(alice())).unwrap_err();
    assert!(matches!(e, Error::NotPermitted(_)));
    // Inactive records are hidden from listings unless asked for.
    let active = repo.list(&RecordQuery::kind(RecordKind::Issue)).unwrap();
    assert!(active.is_empty());
    let all = repo.list(&RecordQuery { include_inactive: true, ..RecordQuery::kind(RecordKind::Issue) }).unwrap();
    assert_eq!(all.len(), 2);
    let tl = repo.timeline(&id).unwrap();
    assert_eq!(tl.iter().map(|t| t.event.as_str()).collect::<Vec<_>>(), ["created", "corrected", "superseded"]);
}

fn redaction_scrubs_history<S: Store>(repo: &mut Repository<S>) {
    let mut fields = ext("roads");
    fields.insert("reporter_phone".into(), ExtValue::Text("+91 98xxxxxx01".into()));
    let src = SourceRef::new("https://photos.example.org/private/abc.jpg").title("my house");
    let src_id = src.id;
    let id = repo
        .create(
            RecordDraft::new(RecordKind::Issue, "Broken drain")
                .body("Call me at +91 98xxxxxx01")
                .source(src)
                .extension(NS, SchemaVersion::new(1, 0), fields),
            Meta::by(alice()),
        )
        .unwrap()
        .record
        .id;
    repo.amend(&id, vec![Change::Body(Some("Still broken, call +91 98xxxxxx01".into()))], Meta::by(alice())).unwrap();

    let paths = vec![
        FieldPath::Body,
        FieldPath::Extension { namespace: NS.into(), field: "reporter_phone".into() },
        FieldPath::Source { id: src_id },
    ];
    assert!(repo.redact(&id, paths.clone(), Meta::by(alice())).is_err(), "reason required");
    repo.redact(&id, paths, Meta::by(alice()).reason("personal data")).unwrap();

    let record = repo.get(&id).unwrap();
    assert_eq!(record.body.as_deref(), Some("[redacted]"));
    assert_eq!(record.extension_field(NS, "reporter_phone"), Some(&ExtValue::Redacted));
    assert!(record.sources[0].locator.starts_with("urn:redacted:"));
    assert_eq!(record.redactions.len(), 3);

    let history = repo.history(&id).unwrap();
    let dump = serde_json::to_string(&history).unwrap();
    assert!(!dump.contains("98xxxxxx01"), "personal data left in history: {dump}");
    assert!(!dump.contains("photos.example.org"));
    // History remains consistent: replaying it yields the current record.
    assert_eq!(pardarsh_core::projection::project(&history).unwrap().unwrap(), record);
    // New values may be written after a redaction.
    repo.amend(&id, vec![Change::Body(Some("Drain overflowing".into()))], Meta::by(alice())).unwrap();
}

fn timestamps_are_not_trusted<S: Store>(repo: &mut Repository<S>) {
    let future = repo.now() + chrono::Duration::days(1);
    let e = repo.create(issue_draft("x"), Meta::by(alice()).occurred_at(future)).unwrap_err();
    assert!(invalid(e).iter().any(|m| m.contains("future")));
    let past = repo.now() - chrono::Duration::days(14);
    let c = repo.create(issue_draft("Light out for two weeks"), Meta::by(alice()).occurred_at(past)).unwrap();
    assert_eq!(c.event.occurred_at, past);
    assert_eq!(c.event.recorded_at, repo.now());
    assert_eq!(c.record.times.recorded_at, repo.now());
}

fn annotations_and_listing<S: Store>(repo: &mut Repository<S>) {
    let a = repo.create(issue_draft("a"), Meta::by(alice())).unwrap().record.id;
    let b = repo.create(RecordDraft::new(RecordKind::Evidence, "b"), Meta::by(alice())).unwrap().record.id;
    let name: NamespacedName = "test.civic:note".parse().unwrap();
    repo.annotate(
        &a,
        name,
        BTreeMap::from([("text".into(), ExtValue::Text("checked".into()))]),
        Meta::by(alice()).visibility(Visibility::private()),
    )
    .unwrap();
    assert_eq!(repo.get(&a).unwrap().version, 2);
    let issues = repo.list(&RecordQuery::kind(RecordKind::Issue)).unwrap();
    assert_eq!(issues.len(), 1);
    let with_ext = repo.list(&RecordQuery { extension: Some(NS.into()), ..Default::default() }).unwrap();
    assert_eq!(with_ext.len(), 1);
    let all = repo.list(&RecordQuery::default()).unwrap();
    assert_eq!(all[0].id, b, "newest first");
    let page = repo.list(&RecordQuery { limit: Some(1), offset: 1, ..Default::default() }).unwrap();
    assert_eq!(page[0].id, a);
}

for_each_store!(
    create_get_and_history,
    rejects_malformed_records,
    extension_references_are_checked,
    optimistic_concurrency,
    idempotency,
    relations_and_graph,
    corrections_retractions_supersession,
    redaction_scrubs_history,
    timestamps_are_not_trusted,
    annotations_and_listing,
);

#[test]
fn sqlite_persists_and_guards_origin() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pd.sqlite");
    let id = {
        let mut repo = Repository::new(SqliteStore::open(&path, origin()).unwrap(), Validator::new(registry()));
        let id = repo.create(issue_draft("persisted"), Meta::by(alice()).idempotency_key("k1")).unwrap().record.id;
        repo.amend(&id, vec![Change::Summary(Some("s".into()))], Meta::by(alice())).unwrap();
        id
    };
    let repo = Repository::new(SqliteStore::open(&path, origin()).unwrap(), Validator::new(registry()));
    let r = repo.get(&id).unwrap();
    assert_eq!(r.version, 2);
    assert_eq!(repo.history(&id).unwrap().len(), 2);
    assert!(repo.store().event_by_idempotency_key("k1").unwrap().is_some());
    drop(repo);
    let other = SqliteStore::open(&path, Origin::new("other.test").unwrap());
    assert!(matches!(other, Err(Error::Storage(_))));
}
