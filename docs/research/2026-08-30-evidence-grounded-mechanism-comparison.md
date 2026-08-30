# Evidence-grounded mechanism comparison

Date: 2026-08-30  
Status: research memo; not product authority

## Question

Should Saccade address a recurring research failure in which an Agent retrieves a
project with similar words or imagery and then incorrectly treats its underlying
mechanism as equivalent?

The motivating example is a game loop with five linked events: the player dies;
the dead player becomes a location-bound boss; that boss persists into a later
encounter; defeating it resolves the encounter; and the same character becomes
playable. A project that shares only "death", "boss", or "character unlock" is
related, but it does not implement the same mechanism.

## Finding

This is a real and well-studied failure class. In the papers and product
documentation reviewed here, however, no source demonstrates a general solution
for cross-project mechanism equivalence. Adjacent work suggests four candidate
components worth testing:

1. retrieve candidates;
2. decompose the proposed equivalence into atomic relational claims;
3. locate evidence for each claim, including temporal evidence in video;
4. classify each claim as supported, refuted, or not established.

These are candidate components synthesized from adjacent work, not a validated
pipeline for this task. A checklist, typed relation set, event graph, or no
additional structure should be compared experimentally.

The evidence supports one product decision now: do not change Saccade's
production contract. It does not yet support choosing atomic claims, causal
graphs, proof capsules, or a companion service as the architecture. The next
step is to reproduce the reported failure in a labeled hard-negative benchmark
and test those techniques as competing hypotheses.

## Review scope and limits

The review sampled primary papers on adversarial semantic similarity, natural
language inference, fact verification, causal or temporal reasoning, and video
temporal grounding. The product sample covered cited web research, scholarly
evidence, video retrieval, and provenance systems whose official documentation
was publicly accessible on 2026-08-30.

This is not an exhaustive market survey. Product documentation cannot establish
undocumented internal behavior, and benchmark results in sentences, scientific
claims, or synthetic videos may not transfer to gameplay or arbitrary project
mechanisms. Claims below are limited to the reviewed sources.

## What the papers establish

### Surface similarity is a known shortcut

[PAWS](https://aclanthology.org/N19-1131/) constructed sentence pairs with high
lexical overlap that were not paraphrases. Models trained on conventional data
scored below 40% on the adversarial set. This closely matches the reported
failure: the same nouns and nearby concepts are treated as equivalent despite a
different relationship or event order.

[HANS](https://aclanthology.org/P19-1334/) showed that strong natural-language
inference models relied on lexical-overlap, subsequence, and constituent
heuristics and failed controlled examples where those heuristics were wrong.
More search and a larger context do not, by themselves, remove the shortcut.

[Gold et al.](https://aclanthology.org/W19-4004/) examined the relationship
between paraphrase and bidirectional entailment. Their results are a warning
against collapsing related semantic relations into a single similarity score.

### Verification requires claims and necessary evidence

[FEVER](https://aclanthology.org/N18-1074/) defines three outcomes—Supported,
Refuted, and NotEnoughInfo—and records the sentences necessary for the first two
judgments. Evidence is part of the task, not decoration attached after a label.

[FActScore](https://aclanthology.org/2023.emnlp-main.741/) decomposes long-form
output into atomic facts and measures the share supported by a reliable source.
This is a better structural precedent than one overall confidence number.

[Retrieval-Augmented Verification](https://aclanthology.org/2024.findings-acl.551/)
reports that heuristic semantic-similarity retrieval returns task-irrelevant
evidence. It trains evidence selection with feedback from the downstream
verifier. The implication is important: citations can still be irrelevant even
when retrieval looks semantically plausible.

[Chen et al.](https://aclanthology.org/2024.naacl-long.196/) use a five-part
pipeline for real-world claims: claim decomposition, raw-document retrieval,
fine-grained evidence retrieval, claim-focused summarization, and veracity
judgment. The authors also note that a reliable evidence summary may remain
incomplete. A product therefore needs an explicit unknown state.

### Video retrieval and causal verification are separate

[UniVTG](https://openaccess.thecvf.com/content/ICCV2023/html/Lin_UniVTG_Towards_Unified_Video-Language_Temporal_Grounding_ICCV_2023_paper.html)
frames video-language grounding as locating the target intervals or shots for a
language query. This supports event-first clip retrieval instead of asking a
model to repeatedly inspect an entire video.

[NExT-QA](https://openaccess.thecvf.com/content/CVPR2021/html/Xiao_NExT-QA_Next_Phase_of_Question-Answering_to_Explaining_Temporal_Actions_CVPR_2021_paper.html)
and [CLEVRER](https://openaccess.thecvf.com/content_CVPR_2020/html/Yi_CLEVRER_CoLlision_Events_for_Video_REpresentation_and_Reasoning_CVPR_2020_paper.html)
treat temporal and causal reasoning as explicit evaluation problems. Finding a
visually similar moment is not sufficient to establish the cause, persistence,
or consequence of an event.

A compact proof capsule is therefore a testable hypothesis, not a settled
design: a few timestamped clips might cover the trigger, intermediate
transformation, later encounter, resolution, and unlock more efficiently than
whole-video inspection. The benchmark must test whether it preserves the
evidence needed for a correct judgment.

## What products currently cover

| Product or standard | Useful capability | Missing for this decision |
|---|---|---|
| [Perplexity Deep Research](https://www.perplexity.ai/help-center/en/articles/13600190-what-s-new-in-advanced-deep-research) | Multi-step research and cited reports | No documented causal-mechanism equivalence contract |
| [NotebookLM](https://blog.google/innovation-and-ai/products/notebooklm-audio-video-sources/) | Source-grounded answers; YouTube citations link to the transcript | Transcript evidence may omit the visual event; no documented edge-by-edge mechanism verdict |
| [Elicit](https://elicit.com/solutions/literature-review) | Literature workflows with sentence-level citations and supporting quotations | Scientific literature scope, not arbitrary web projects or gameplay mechanisms |
| [Consensus Research Agent](https://help.consensus.app/en/articles/12641232-research-agent) | Academic search, study comparison, and evidence synthesis | No general cross-project causal equivalence evaluator |
| [scite](https://scite.ai/) | Citation contexts classified as supporting, contrasting, or mentioning | Classifies scholarly citation relations, not product mechanisms |
| [Twelve Labs](https://docs.twelvelabs.io/v1.2/docs/guides/generate-text-from-video/summaries-chapters-and-highlights) | Video search, chapters, highlights, and timestamped segments | Locates evidence but does not prove that two causal graphs are equivalent |
| [W3C PROV](https://www.w3.org/TR/prov-overview/) and [C2PA](https://c2pa.org/) | Provenance and content history | Provenance records origin; it does not establish semantic truth or entailment |

The reviewed products expose useful components for candidate retrieval,
source-grounded writing, scholarly evidence classification, and timestamped
video retrieval. Their public documentation does not describe a
domain-independent contract for cross-project mechanism equivalence. That is a
gap in this sample, not proof that no product or unpublished system has one.

## Fit with Saccade's current boundary

Saccade already provides several useful primitives:

- exact tab, document, revision, and object identity;
- explicit full or delta reads rather than an unscoped browser summary;
- bounded semantic working sets;
- explicit opaque and restricted surfaces;
- no silent replacement of stale evidence;
- no screenshots, DOM dumps, or persistent page-secret storage.

Where the semantic projection is complete, those properties can make a browser
observation traceable to one current tab basis. They do not make the observation
durable or sufficient research evidence. The Broker deliberately does not
retain canonical Truth across restart, object identities expire at document
replacement, and `truth.read` is exact-tab rather than a cross-source knowledge
API. Screenshots and editable values are excluded; restricted frames, Canvas,
WebGL, and ordinary video may be opaque.

Putting comparison, persistent evidence, and video interpretation into the
Broker would mix three products and weaken the privacy and exact-tab design. A
document or object reference alone cannot serve as durable evidence after
navigation or restart.

## Options

### A. Add comparison to Saccade core now

Not recommended. There is no benchmark, stable schema, or evidence that one
model-independent comparison contract is ready. It would also pressure the
six-tool API, exact-tab boundary, and no-persistence guarantees.

### B. Build a separate evidence-grounding product

Not yet justified. One plausible pipeline would be:

```text
candidate retrieval
→ mechanism/event decomposition
→ required-edge checklist
→ text or timestamped-video evidence retrieval
→ support / refute / unknown per edge
→ causal-graph comparison
→ equivalent / related / contradicted / insufficient evidence
```

Every arrow is an unvalidated architectural choice. In particular, an atomic
claim checklist may be sufficient without a causal graph, and a domain-specific
schema or human adjudication may outperform a general mechanism ontology.

### C. Buy a research product and treat its answer as the verdict

Not recommended. Existing products can reduce retrieval and review cost, but
their documented contracts do not close the equivalence gap. They should be
evidence suppliers or analyst tools, not final authority.

### D. Reproduce and benchmark without building a product

Recommended. Use the existing six Saccade tools only where their semantic Truth
contains the relevant public evidence. Use ordinary research inputs for other
sources, and keep annotation and evaluation outside the Broker. This option can
establish the task, baseline, failure rate, and useful ablations without
committing to a new protocol, service, storage model, or vendor.

## Experiment before a product decision

1. Collect real incidents before designing the schema. Preserve the original
   query, returned answer, cited sources, expected judgment, and exact reason
   the judgment was wrong. Remove secrets and obtain permission for retained
   media.
2. Write annotation rules for `equivalent`, `related`, `contradicted`, and
   `insufficient evidence`. Have at least two people label a pilot set and
   measure agreement before scaling it.
3. Assemble hard negatives with similar vocabulary, imagery, or genre but
   different trigger, ordering, persistence, actor, or outcome. Keep a held-out
   set that is not used while changing prompts or schemas.
4. Reproduce the false-equivalence rate for ordinary search summaries and cited
   research answers. A new system is unnecessary if the incident is rare or a
   smaller decision-policy change fixes it.
5. Run ablations: retrieval only; citations; atomic claims; atomic claims plus
   explicit unknown; timestamped clips; event graphs; and human adjudication.
   Do not assume the most elaborate pipeline wins.
6. Measure false-equivalence rate first. Also measure false contradiction,
   evidence coverage, citation correctness, abstention calibration, annotator
   agreement, human review time, inspected video seconds, latency, and cost.
7. Before retaining evidence, specify consent, source licensing, untrusted-page
   handling, redaction, retention, deletion, and access isolation. Saccade's
   ephemeral object identities must never be presented as durable provenance.
8. Propose a product or core change only if the benchmark identifies a reusable
   primitive that multiple Agent stacks need and that preserves Saccade's
   privacy and isolation contracts.

## Provisional decision

Do not change Saccade's production contract and do not authorize a companion
product yet. Authorize only incident collection, annotation rules, and a
hard-negative benchmark. Atomic verification, event graphs, timestamped proof
capsules, external products, and human review are experimental arms. Revisit a
build, buy, or Saccade-integration decision after the pilot reports measured
error, agreement, cost, privacy constraints, and ablation results.
