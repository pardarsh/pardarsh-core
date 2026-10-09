//! A second, non-issue consumer of pardarsh-core: a bill proposing a change
//! to a legal provision, with an objection, a response, a committee decision
//! and the amended provision superseding the old one.
//!
//! Run: `cargo run --example legislative_change [-- --bundle out.json]`
//!
//! Everything here is hypothetical and uses only core record kinds and
//! relations; no application crate is involved.

use chrono::{TimeZone, Utc};
use pardarsh_core::graph::{self, TraversalOptions};
use pardarsh_core::interchange;
use pardarsh_core::prelude::*;

fn main() -> Result<()> {
    let origin = Origin::new("legis.example.org")?;
    let clock = ManualClock::new(Utc.with_ymd_and_hms(2026, 3, 2, 9, 0, 0).unwrap());
    let mut repo = Repository::new(MemoryStore::new(origin), Validator::default()).with_clock(clock.clone());
    let clerk = Attribution::system("clerk");
    let day = chrono::Duration::days(1);

    let legislature =
        repo.create(RecordDraft::new(RecordKind::Actor, "State Legislative Assembly"), Meta::by(clerk.clone()))?.record;
    let member =
        repo.create(RecordDraft::new(RecordKind::Actor, "Member, Constituency 12"), Meta::by(clerk.clone()))?.record;
    let committee = repo
        .create(RecordDraft::new(RecordKind::Actor, "Standing Committee on Urban Affairs"), Meta::by(clerk.clone()))?
        .record;
    let as_member = Attribution::public(ActorRef::Record { id: member.id.clone() });
    let as_committee = Attribution::public(ActorRef::Record { id: committee.id.clone() });

    let provision = repo
        .create(
            RecordDraft::new(RecordKind::Source, "Municipal Act, s. 42: street lighting duty")
                .body("The municipality shall maintain street lighting in public streets.")
                .source(
                    SourceRef::new("https://legis.example.org/acts/municipal/s42")
                        .issuer("State Legislative Assembly")
                        .authority(SourceAuthority::Official)
                        .retrieved_at(repo.now()),
                )
                .relation(RelationDraft::new(RelationKind::IssuedBy, legislature.id.clone())),
            Meta::by(clerk.clone()),
        )?
        .record;

    clock.advance(day);
    let bill = repo
        .create(
            RecordDraft::new(RecordKind::Proposal, "Bill 17: repair deadline for street lighting")
                .summary("Adds a 7-day repair deadline to s. 42.")
                .actor(ActorRoleKind::Author, as_member.clone())
                .relation(RelationDraft::new(RelationKind::ProposesChangeTo, provision.id.clone())),
            Meta::by(as_member.clone()),
        )?
        .record;

    clock.advance(day);
    let objection = repo
        .create(
            RecordDraft::new(RecordKind::Comment, "Objection: 7 days is infeasible in monsoon")
                .relation(RelationDraft::new(RelationKind::RespondsTo, bill.id.clone()))
                .relation(RelationDraft::new(RelationKind::Disputes, bill.id.clone())),
            Meta::by(as_committee.clone()),
        )?
        .record;
    repo.create(
        RecordDraft::new(RecordKind::Comment, "Response: amend to 14 days during declared emergencies")
            .relation(RelationDraft::new(RelationKind::RespondsTo, objection.id.clone())),
        Meta::by(as_member.clone()),
    )?;
    repo.amend(
        &bill.id,
        vec![Change::Summary(Some(
            "Adds a 7-day repair deadline (14 days during declared emergencies) to s. 42.".into(),
        ))],
        Meta::by(as_member.clone()).reason("amendment after committee objection"),
    )?;

    clock.advance(day * 10);
    let decision = repo
        .create(
            RecordDraft::new(RecordKind::Decision, "Committee recommends Bill 17 as amended")
                .relation(RelationDraft::new(RelationKind::IssuedBy, committee.id.clone())),
            Meta::by(as_committee.clone()),
        )?
        .record;
    repo.link(&bill.id, RelationDraft::new(RelationKind::DecidedBy, decision.id.clone()), Meta::by(clerk.clone()))?;

    // Passage is a formal legal event; here it is only recorded, not decided by the system.
    clock.advance(day * 20);
    let amended = repo
        .create(
            RecordDraft::new(RecordKind::Source, "Municipal Act, s. 42 (as amended by Act 17)")
                .body("The municipality shall maintain street lighting in public streets and repair faults within 7 days (14 during declared emergencies).")
                .relation(RelationDraft::new(RelationKind::DerivedFrom, provision.id.clone())),
            Meta::by(clerk.clone()),
        )?
        .record;
    repo.link(
        &bill.id,
        RelationDraft::new(RelationKind::ResultedIn, amended.id.clone()).note("enacted text resulting from the bill"),
        Meta::by(clerk.clone()),
    )?;
    repo.supersede(&provision.id, &amended.id, Meta::by(clerk.clone()).reason("Act 17 in force"))?;

    println!("Timeline of {}:", bill.title);
    for t in repo.timeline(&bill.id)? {
        let changes: Vec<String> = t.changes.iter().map(|c| c.field.clone()).collect();
        println!(
            "  #{} {} at {} [{}] {}",
            t.sequence,
            t.event,
            t.recorded_at.date_naive(),
            changes.join(", "),
            t.reason.unwrap_or_default()
        );
    }

    println!("\nConnected records within two hops of the bill:");
    let traversal = graph::traverse(repo.store(), &bill.id, &TraversalOptions::default())?;
    for (id, depth) in &traversal.nodes {
        let r = repo.get(id)?;
        println!("  [{depth}] {} {}", r.kind, r.title);
    }
    for e in &traversal.edges {
        println!("  {} --{}--> {}", repo.get(&e.source)?.title, e.relation.kind, repo.get(&e.relation.target)?.title);
    }
    println!("\nProvision lifecycle: {:?}", repo.get(&provision.id)?.lifecycle);

    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--bundle") {
        let ids: Vec<RecordId> = repo
            .list(&RecordQuery { include_inactive: true, ..Default::default() })?
            .into_iter()
            .map(|r| r.id)
            .collect();
        let bundle = interchange::export(&repo, &ids, &Viewer::privileged())?;
        std::fs::write(
            &args[i + 1],
            bundle.to_json_pretty()?
                + "
",
        )
        .map_err(|e| Error::Storage(e.to_string()))?;
        println!("wrote {}", args[i + 1]);
    }
    Ok(())
}
