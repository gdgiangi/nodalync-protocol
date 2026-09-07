# Draft RFC: collective commitments across independently operated agents

Status: research proposal for review, 7 September 2026. This document does not
change Nodalync's implementation or claim a deployed protocol. The message names
below are proposed semantics, not an implemented API. No commitments, purchases,
recruitment messages, or transfers have been made.

## The proposal

Build an interoperable protocol through which people and organizations can
authorize mutually dependent parts of a shared plan. A commitment says:

> I authorize this specific contribution, within these limits, if this complete
> set of commitments is activated under these rules before this deadline.

The target is a coordination problem: people can prefer an attainable joint
outcome while declining to act alone. Each participant needs assurances about
other participants' contributions. The protocol would make those dependencies
explicit, discover compatible contributions, and establish a common decision
about which authorized obligations activate.

The ambition is to make cooperation affordable across independently controlled
organizations and their agents. The outcome worth demonstrating is a completed,
mutually acceptable project whose coordination costs previously prevented it.
Scientific novelty, practical value, and adoption are separate claims; none has
yet been established for this proposal.

## A concrete first use case

Several organizations want to adopt an open software system. Their willingness
depends on migration tooling, continuing maintenance, and support being available.
A maintainer can build the tooling if enough funding is committed. A support
provider can reserve capacity if enough customers commit. Each party has a
reasonable reason to wait.

For a deliberately simplified demonstration, suppose six organizations each
offer 1,000 accounting units, a maintainer requires 4,000, and a support provider
requires 2,000. These are synthetic amounts, not prices or revenue estimates.
The actual agreement must specify deliverables, license, acceptance tests,
service period, deadlines, payment schedule, cancellation, and failure handling.

Each customer makes its contribution conditional on the same plan containing
the other required contributions and accepted provider obligations. Providers
make their obligations conditional on the agreed funding arrangement. The plan
can activate all these obligations together. It cannot guarantee that future
software or support will be delivered successfully.

A central organizer could already arrange this. The protocol's proposed value is
allowing existing customer agents, maintainer services, support businesses, and
organizers to participate through compatible implementations. A participant
should not need a separate bespoke integration for every coalition organizer.

The initial experiment should stay within this one domain. Cross-domain projects
such as shared energy infrastructure are possible later applications, not
capabilities established by this RFC.

## Why AI belongs in this system

Agents could reduce the cost of eliciting constraints, finding complementary
offers, checking documents, drafting candidate plans, and explaining why a plan
fails. For example, an agent might discover that extending a migration deadline
makes an otherwise unaffordable support arrangement feasible.

Model output is a proposal. It does not constitute a principal's consent, prove
that a resource exists, or authorize spending. Each participant's approved policy
or explicit approval determines whether an exact plan is acceptable. Natural
language explanations accompany typed terms; a model-generated interpretation
cannot silently change those terms.

The model can perform expensive, fallible search. Narrow deterministic validators
check supported constraints and authorization. A validator can establish
conformance to represented conditions; it cannot establish that the representation
faithfully captures every human concern. Ambiguous or unrepresented terms require
human resolution.

## Protocol boundary

Use existing transport, authentication, and resource-specific execution systems.
[A2A](https://a2a-protocol.org/latest/specification/) supplies an existing carrier
and extension mechanism. The proposal belongs initially in an application profile
or extension, rather than a new transport network.

The protocol would standardize:

- The distinction between interest, a proposal, a reservation, and an activated
  obligation.
- References to the exact plan, authorized principal, applicable terms, and
  resource reservation.
- Mutually conditional activation, amendment, expiry, and decision recovery.
- How execution evidence and failure events refer to the activated agreement.
- Conformance cases that independently built implementations must interpret alike.

It would leave local planning models, private preferences, search algorithms,
identity providers, payment rails, storage engines, and commercial policy to
participants and adapters. A particular pilot can use a trusted coordinator;
the record must state that trust assumption.

## Objects and messages

All signed objects need canonical encoding, domain-separated signatures, issuer
identity, authority scope, version, expiry where applicable, and replay
protection. Exact cryptographic profiles remain to be selected and reviewed.

| Object or message | Meaning |
| --- | --- |
| `ADVERTISE` | Discoverable capability or interest, with a domain profile and contact endpoint. Nonbinding. |
| `OFFER` | Versioned terms under which a principal is willing to consider participation. Authority determines whether any stronger commitment is permitted. |
| `PLAN` | Exact participants, contributions, dependencies, terms, verification rules, decision service, execution adapters, and failure handling. Identified by a canonical digest. |
| `PREPARE` | A request to validate one exact plan and obtain the approvals and reservations it requires. |
| `READY` | Evidence that a participant has authorized that plan and obtained specified reservations under the agreed decision rules. This is not proof of future delivery. |
| `DECIDE` | A durable commit or abort decision from the plan's accepted decision service. A commit requires all required valid readiness records. |
| `EXECUTION_EVENT` | A scoped observation about an obligation: started, evidence submitted, accepted, failed, disputed, or compensated. |

An offer can participate in discovery for several alternative plans. Readiness
for one plan must respect exclusive resource reservations and cannot implicitly
authorize another. Any change to price, scope, counterparty, dependency, or other
binding term produces a new plan digest and requires appropriate authorization.

The v0.1 domain profile should restrict activation conditions to defined predicates
over the final plan and verified reservations: named participants, required roles,
minimum funded amount, contribution caps, compatible license terms, and specified
deadlines. It should not accept arbitrary remote executable predicates or claim
universal semantic compatibility.

Mutual conditions must refer to obligations present in the same candidate plan,
not require each other to have already activated. Otherwise a cycle of reasonable
conditions becomes an impossible sequence. Solving a supported set of constraints
does not establish a globally optimal allocation or truthful preference revelation.

## Activation and failure semantics

The first implementation should have a plan-specific decision service with a
durable, single authoritative ordering of commit and abort decisions. It can be
operated by an agreed trusted coordinator. Replication and replacement require a
specified consensus and recovery design; signatures alone do not supply one.

The intended sequence is:

1. Discover offers and construct an exact candidate plan.
2. Obtain each principal's authorization and resource-specific reservations.
3. Validate every required readiness record against that same plan.
4. Record one final decision under the agreed deadline and reservation rules.
5. Let adapters observe that decision and execute idempotently.
6. Record fulfillment evidence, acceptance, failure, or compensation.

The safety goal is that conforming executors never activate an obligation without
the required common authorization decision. This is conditional on the decision
service and resource adapters satisfying their contracts. It is not a guarantee
against a malicious coordinator, dishonest resource issuer, or compromised key.

During a network partition, an executor can be unable to determine whether a
commit already occurred. It must enter a pending recovery state. A local timeout
alone cannot safely release an exclusive reservation or establish that the plan
aborted. The profile must define how deadlines interact with the authoritative
decision, how reservations remain valid for an already committed plan, and how
an abort is established. Losing availability in an uncertain state may be necessary
to preserve safety.

The profile must also define withdrawal between readiness and the final decision.
It must either make readiness irrevocable for the agreed decision period or let an
authorized withdrawal, ordered before commit by the decision service, force abort.
A local revocation cannot immediately release a reservation while a partitioned
coordinator can still commit using the previous readiness record. These semantics
must be visible to the principal before readiness is authorized.

There are three distinct execution cases:

- Resources under a common transactional authority may support atomic changes.
- Independent services may support reservations followed by idempotent execution,
  with explicitly defined recovery and compensation.
- Physical work and human services remain future obligations with agreed evidence,
  remedies, and institutions responsible for resolving disputes.

Atomic activation of obligations must never be presented as atomic fulfillment
of a real-world project. An escrow balance cannot prove that a software migration
will succeed. Legal effect also depends on the agreement and applicable
institutions; this RFC does not establish enforceability.

## Querying, discovery, and storage

A participant's agent queries organizers or compatible directories for offers
matching a domain, capability, timing, and disclosed constraints. The host software
must expose and invoke that capability. Installing a protocol endpoint alone does
not make an arbitrary agent discover or use it.

Discovery responses include versioned offer references and minimal public terms.
The agent can request additional terms from their owners, propose a plan, and
receive structured reasons for rejection when the participant permits disclosure.
The agent's host places these messages into its context as external data.
Authorization checks run separately from the language model.

Each participant stores its private preferences, mandates, and records locally or
within its organization. Directories store intentionally disclosed advertisements.
An organizer stores the shared plans and decision records for coalitions it
coordinates. Authorized participants retain independently verifiable copies of
the records relevant to them. SQL databases and ordinary artifact storage are
sufficient for the first version.

Keeping private reservation values in a local database does not prevent a
counterparty from learning about them through repeated queries. The first pilot
should make a limited disclosure claim and bound negotiation rounds. Stronger
privacy needs a separate threat model, mechanisms, and measurements.

## Close predecessors and the burden of differentiation

The following are substantive predecessors, not incidental similarities.

| Predecessor | What it already covers | Consequence for this proposal |
| --- | --- | --- |
| [FIPA Contract Net](https://www.fipa.org/specs/fipa00029/SC00029H.html) | Calls for proposals, participant conditions, selection, and task execution. | Agent bidding and conditional proposals are not new. |
| [Tosca](https://www.ijcai.org/proceedings/2017/0037.pdf) | Decentralized commitment protocols over information exchange. | Formal commitment semantics have substantial prior research. |
| [Anoma](https://github.com/anoma/whitepaper/blob/main/whitepaper.md) | Signed partial intents, counterparty discovery, solver composition, and valid state transitions. | Composing independently supplied constraints is a close architectural precedent. Reuse or integration should be assessed before inventing equivalent machinery. |
| [Clearing Conditional Commitments](https://github.com/mini-matters/clearing-conditional-commitments) | A June 2026 working paper models a clearinghouse for interdependent action and its disclosure, specification, and custody choices. | Even the broad social thesis has close prior art. Its theoretical claims are not deployment evidence for this RFC. |
| [A202](https://a202.org/schemas/canonical-commercial-model-v0.1/) | An experimental commercial model for authority, negotiation, conditional commitments, agreements, execution evidence, and settlement handoffs. | Generic signed agreement objects are insufficient differentiation. Its stated scope is synthetic pilot transactions. |
| [A2A](https://a2a-protocol.org/latest/specification/) and [AP2](https://ap2-protocol.org/ap2/specification/) | Agent interaction and extensions; scoped payment mandates, respectively. | Transport and payment authorization should be reused where appropriate. Neither a task response nor a payment mandate proves successful collective fulfillment. |

The defensible proposed contribution is a narrowly specified, interoperable
profile for discovering and activating complementary multi-party commitments,
with actual adoption across separately operated software. This is a hypothesis
about integration and use, not a claim that a new coordination primitive has been
invented. If an existing protocol supports the required semantics and failures,
implement this as its profile or contribute missing pieces upstream.

## Adoption and the first decisive demonstration

Start with one prospective organizer and one actual shared dependency. The
organizer should already have access to potential customers and providers; a new
public network with no participants would not test the useful part of the thesis.
Recruitment would require separate explicit authorization before contacting anyone.

Participant incentives are concrete: customers obtain a feasible shared service,
maintainers see committed demand, and support providers can plan capacity.
An organizer can charge a disclosed coordination fee if participants accept it.
That fee needs a line item in the plan. No attribution formula or token issuance
is required to test these incentives.

The human interface shows the exact obligation, maximum exposure, activation
conditions, deadline, and failure terms. The agent interface supplies typed objects,
predictable errors, resumable state, and conformance fixtures. Both must refer to
the same authorized plan.

The demonstration must include independently implemented participant clients.
Interoperability among copies of one application is weak evidence. An existing
participant should be able to join a second organizer's compatible coalition
without a custom bilateral adapter or migrating its private preferences.

Compare against competent manual coordination, a centralized coordination
application, and an implementation using the closest existing protocol. Measure:

- Real project completion and participant outcomes, including failed projects.
- Human coordination hours and total operating cost.
- Integration effort for an additional independently built client or organizer.
- Unauthorized, duplicate, stale, or conflicting commitments.
- Recovery behavior after crashes, expiry, revocation, and network partitions.
- Whether users understand and approve the actual obligations they undertake.

Separate the value of AI assistance from the value of interoperability. The same
planning model should be available to relevant comparison systems. A superior
model or subsidized organizer is not evidence that the protocol caused the gain.
The centralized comparison should expose a competent documented API and receive
the same domain adapters. Measure incremental integration and switching effort
across independently operated organizers; comparing against an artificially closed
application would not establish the benefit of a shared profile.

Synthetic conformance tests should cover changed plan terms, mismatched digests,
duplicate messages, conflicting reservations, revoked authority before readiness,
withdrawal between readiness and decision, late readiness, uncertain final
decisions, malicious execution receipts, and
failed fulfillment after valid activation. No performance or safety results are
reported here: these tests have not yet been implemented or run.

## Conditions for stopping or changing direction

- If a close existing specification already meets the requirements, contribute
  an implementation or profile rather than launch a redundant standard.
- If domain-specific integrations dominate the work after a shared profile exists,
  the proposed interoperability advantage may be too small.
- If participants will not authorize useful conditional contributions, discovery
  and solvers cannot manufacture demand.
- If an ordinary organizer achieves equivalent outcomes and participation costs,
  a separate protocol needs evidence of portability or ecosystem value to justify
  its complexity.
- If obtaining reliable reservations or resolving failures is institutionally
  infeasible, a successful message exchange is not a successful system.

The social claim must remain bounded. Voluntary agreement among participants does
not establish fairness to people outside the agreement. A coalition can have
external costs, unequal bargaining power, or exclusionary effects. The protocol
must not label constraint satisfaction as social welfare or fair attribution.

## Relationship to Nodalync and the review decision

Nodalync supplies experience implementing identifiers, signed messages, persistent
records, networking, and agent interfaces. None of its current attribution or
payment semantics establishes the authorization and activation rules above.
Reusing its settlement path would require resolving the independent integrity
findings already under review.

This RFC proposes a separate research direction. The next decision is whether to
investigate a collective-adoption profile on top of existing commitment and agent
protocols, with one domain partner and independently built clients. It is not a
request to merge a protocol redesign into the current implementation.
