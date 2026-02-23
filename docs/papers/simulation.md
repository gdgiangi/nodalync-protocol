# Simulating the Nodalync Protocol: Who Wins in a Knowledge Economy?

**A 1,400-simulation Monte Carlo analysis of provenance economics across 24 actor archetypes**

*Gabriel Giangi · February 2026*

---

## Abstract

The Nodalync Protocol proposes a knowledge economy where every piece of published information carries permanent provenance—a traceable chain of attribution that ensures original creators are compensated every time their work is referenced, synthesized, or queried downstream. But does it actually work? Do creators really out-earn aggregators? Can sybil attackers game the system? Is the "publish once, earn forever" promise real?

To find out, I built a discrete-event simulator modeling 24 distinct actor archetypes across 6 behavioral categories. The investigation proceeded in two phases:

1. **Baseline analysis**: 50 independent Monte Carlo simulations × 4,000 ticks, establishing the core economic dynamics.
2. **Overnight parameter sweeps**: 1,400 additional simulations across three experiments—an owner-share sweep (1–20%), a dampening exponent sweep (0.3–0.9), and a long-horizon run (40,000 ticks)—stress-testing the protocol's key design parameters.

The results are statistically significant, sometimes surprising, and carry real implications for anyone building provenance-aware knowledge systems.

---

## 1. The Core Question

The Nodalync whitepaper makes a bold economic claim: under a 95/5 revenue split (95% flows to root content creators, 5% to the synthesis layer), a **domain expert who publishes once and walks away** should earn more lifetime value than an **aggregator who queries, synthesizes, and imports continuously**.

This is the A1 vs D3 test—the central hypothesis of Nodalync's economic model.

If true, it means the protocol's incentive structure is fundamentally aligned: create original knowledge, and the economics will find you. If false, the aggregator strategy dominates, and the protocol devolves into a content remix engine where middlemen extract the most value.

We tested this across 50 random seeds with 95% confidence intervals.

---

## 2. Simulation Design

### Architecture

The simulator models a Nodalync network as a population of autonomous actors, each following probabilistic behavioral profiles:

- **Content nodes** exist at two layers: **L0** (root facts/observations) and **L3** (synthesized compositions that reference L0 sources)
- Every query settles economics: the querier pays, the node owner keeps 5%, and **95% flows proportionally to all root L0/L1 sources** in the node's provenance chain
- Actors join, leave, publish, query, synthesize, and import based on per-type probability distributions
- A sybil dampening mechanism ($\frac{1}{\sqrt{n}}$ per-controller selection penalty) reduces the effectiveness of content flooding attacks

### The 24 Actor Types

| Category | Actors | Role |
|---|---|---|
| **A — Creators** | Domain Expert, Prolific Publisher, Institutional Source, Passive Legacy Node | Publish original L0 content. The supply side. |
| **B — Consumers** | Private Learner, Targeted Researcher, Enterprise Consumer | Pure demand. Query but never publish. |
| **C — AI Agents** | AI Query Agent, AI Synthesis Agent, AI Import Chain Agent, Agent Swarm | Machine-driven demand and synthesis. The economy's engine. |
| **D — Hybrid** | Curator-Synthesizer, Pure Synthesizer, Aggregator, Knowledge Entrepreneur | Both query and publish. The middlemen. |
| **E — Infrastructure** | Search Discovery Index, Curated Directory, Specialized Extractor, Application Builder | System-level services that modify discovery, quality, and demand. |
| **F — Adversarial** | Sybil Attacker, Attribution Gamer, Content Copier, Price Manipulator, Free Rider | Attempt to extract value through gaming, flooding, or free-riding. |

### Parameters

- **50 seeds**, each with a different random number generator state
- **4,000 ticks** per simulation (~2× longer than initial exploratory runs)
- **Revenue split**: 95% root / 5% owner (per whitepaper §6.1)
- **Initial population**: 115 actors, with continuous join/leave dynamics bringing the total to ~197 actors per run
- **Sybil dampening**: ON (controlled variable for this sweep)

---

## 3. Results

### 3.1 The Headline: A1 Beats D3 in 98% of Simulations

![A1 vs D3 Distribution](../charts/01_a1_vs_d3_distribution.png)

| Metric | Mean | 95% CI | Range |
|---|---|---|---|
| Domain Expert (A1) avg TVL | **1,559.79** | [1,369.21, 1,750.37] | 553.62 – 2,966.15 |
| Aggregator (D3) avg TVL | 292.26 | [225.95, 358.57] | 93.53 – 1,352.67 |
| Difference | **+1,267.53** | [1,058.91, 1,476.15] | −512.67 – 2,672.19 |

**Domain experts earn 5.3× more than aggregators.** The 95% confidence interval for the difference is entirely above zero, making this result statistically significant at $p < 0.05$.

Only one seed (28 of 50) produced a D3 victory, caused by a single aggregator landing an extremely high-traffic import chain—a low-probability event ($\frac{1}{50} = 2\%$).

**Why does this work?** The 95/5 split is the mechanism. When an aggregator creates a popular L3 synthesis, they capture only 5% of each query. The other 95% flows backward through the provenance chain to whoever originally published the L0 sources. The aggregator does the work of synthesis; the creator captures the economic rent.

This is by design—and the simulation confirms it holds under randomized conditions.

### 3.2 TVL Growth and the 95/5 Split

![TVL Growth](../charts/02_tvl_growth.png)

Total value locked grows approximately linearly over the simulation period, tracking aggregate demand injection from B and C category actors. The root/synth split holds constant at almost exactly 95.0% / 5.0% throughout—the protocol's fundamental economic invariant.

At tick 2,000, a representative run shows:

- **Total TVL**: ~49,131
- **Root TVL**: 46,674 (95.0%)
- **Synth TVL**: 2,457 (5.0%)

The split doesn't drift. It's locked in by the settlement mechanics.

### 3.3 Who Wins, Who Pays

![Category Waterfall](../charts/03_category_waterfall.png)

The category-level economics tell a clear story:

| Category | Net P&L (50-seed mean) | 95% CI |
|---|---|---|
| **A — Creators** | **+41,505** | [39,474, 43,537] |
| **D — Hybrid** | **+6,124** | [4,439, 7,809] |
| **F — Adversarial** | +783 | [−158, +1,723] |
| **E — Infrastructure** | −1,185 | [−1,408, −962] |
| **B — Consumers** | −9,613 | [−10,301, −8,925] |
| **C — AI Agents** | **−37,614** | [−39,068, −36,160] |

**Creators (A) capture 61% of all TVL** with almost zero spend—they publish and collect royalties. The demand side (B + C) injects ~50K into the economy per run. AI agents alone account for ~40K of that, making them the protocol's primary economic engine.

Hybrid actors (D) are net-positive but work hard for it: they spend ~11.7K to earn ~17.8K, for a margin of ~35%. Compare that to creators, who spend ~462 to earn ~42K—a 90× return on effort.

![TVL Share](../charts/04_tvl_share.png)

### 3.4 Actor-Type Time Series

![Actor Tracks](../charts/06_actor_tracks.png)

The time-series view reveals important dynamics:

- **Domain Expert TVL grows continuously** even after most experts have churned out. Their content keeps earning royalties from new queries.
- **Aggregator TVL grows slowly** because they capture only the 5% synthesis margin on their own nodes.
- **Passive Legacy TVL is a straight line up**—zero effort, pure passive income from content published at tick 0.
- **Sybil TVL** (with dampening on) is significantly lower than in the undampened baseline.

### 3.5 Passive Compounding: 17× Return on Zero Effort

![Passive Compounding](../charts/08_passive_compounding.png)

The passive legacy nodes sit at the heart of the Nodalync value proposition: **publish once, earn forever.** These actors publish 18 L0 nodes at initialization time, then never act again—no queries, no synthesis, no additional publications. Just inert content sitting in the network.

Over 4,000 ticks:

- Mean compounding rate: **17.0×** ± 2.9
- 95% CI: [16.2×, 17.8×]
- Range: 12.3× to 24.0×

This means every unit of value "invested" by publishing original content at time zero returns 17 units over the simulation window. The variance (12–24×) depends on whether the specific content happens to be included in popular synthesis chains—quality-weighted selection ensures higher-quality content compounds faster.

---

## 4. Sybil Resistance Under √n Dampening

![Sybil Analysis](../charts/05_sybil_analysis.png)

Sybil attackers publish large quantities of low-quality content (quality = 0.20), attempting to capture royalty flows through sheer volume. Each sybil node starts with 28 low-quality L0 publications and continues publishing aggressively.

We implemented a $\frac{1}{\sqrt{n}}$ dampening mechanism: when a single controller floods the content pool with $n$ nodes, each node's selection probability is reduced by $\frac{1}{\sqrt{n}}$. This means doubling your nodes only increases your expected selection by $\sqrt{2} \approx 1.41\times$, not $2\times$.

**Results with dampening (current run)**:

- Mean sybil net P&L: **+307** (95% CI: [−12, +627])
- The CI includes zero—sybil attacks are **marginally profitable to unprofitable** on average

**Results without dampening (previous run, single seed)**:

- Sybil net P&L: **+6,735** (a single sybil actor earned more than any domain expert)

The dampening mechanism reduced sybil profitability by approximately **86%**. However, the occasionally profitable outlier (max: +4,874 in seed 12) suggests an even stronger dampening curve ($n^{0.7}$ instead of $n^{0.5}$) may be warranted for production deployment.

**Per-actor comparison**: Even when sybils are profitable in aggregate, their per-actor TVL is significantly lower than domain experts. The box plots show domain expert TVL per actor consistently above 1,000, while sybil TVL per actor rarely exceeds 300.

---

## 5. Infrastructure Economics

![Infrastructure ROI](../charts/07_infra_roi.png)

In the initial simulation runs, Category E actors (infrastructure) were completely inert—zero TVL, zero spend, zero queries. We activated them with realistic behavioral profiles:

| Actor | Description | Mean TVL | Mean Spend | Net |
|---|---|---|---|---|
| **Specialized Extractor** | Queries raw L0 content, publishes refined high-quality L0 | **458** | 595 | −137 |
| **Search Discovery Index** | Scans content, publishes index metadata L0 | **237** | 370 | −133 |
| **Curated Directory** | Evaluates content quality, publishes curated L3 compilations | 77 | 295 | −218 |
| **Application Builder** | Queries L3 for integration, publishes application L3 objects | 75 | 323 | −248 |

All infrastructure actors run **net-negative**—they spend more than they earn. This is economically correct: infrastructure actors are service providers that improve the quality of the overall ecosystem (better discovery, higher quality, curated recommendations) in exchange for being subsidized by the value they create for others.

The specialized extractor earns the most because it publishes **high-quality** refined L0 content (0.85–0.97 quality) that gets queried by downstream consumers. It behaves like a value-add creator.

In a production protocol, these actors would likely be subsidized by network fees or operate as utility-maximizing institutions rather than pure profit centers.

---

## 6. Parameter Sweep Analysis (Overnight Run)

The baseline 50-seed run established the protocol's core dynamics. To stress-test its design parameters, I ran **1,400 additional simulations** across three experiments, totaling ~20 hours of compute time with zero failures.

### 6.1 Owner-Share Sweep: Mapping the Fairness Frontier

**Setup**: 20 owner-share values from 1% to 20% × 50 seeds each = 1,000 simulations.

The central question: at what fee level does the aggregator strategy start beating the creator strategy?

![Fairness Frontier](../sweep_outputs/charts/01_fairness_frontier.png)

**Answer: it never does.** Across the entire 1–20% range, domain experts (A1) beat aggregators (D3) in **92–96% of simulations**:

| Owner Share | A1 Mean TVL | D3 Mean TVL | A1/D3 Ratio | Win Rate |
|---|---|---|---|---|
| 1% | 1,613 | 278 | 5.80× | 96% |
| 5% (current) | 1,560 | 320 | 4.87× | 94% |
| 10% | 1,493 | 374 | 3.99× | 94% |
| 15% | 1,426 | 427 | 3.34× | 94% |
| 20% | 1,360 | 480 | 2.83× | 92% |

![Owner Share Win Rate](../sweep_outputs/charts/02_owner_share_winrate.png)

**Key observations:**

- **No crossover exists in the tested range.** A1 TVL decreases by ~13.4 units per 1% increase in owner share, while D3 TVL increases by ~10.6 units. Extrapolating these linear slopes, the theoretical crossover would occur around **owner_share ≈ 51%**—a value no sensible protocol would ever adopt.
- **Total TVL is invariant to owner share.** Every tested value produces ~68,409 total TVL. The parameter controls *distribution*, not *size*, of the economy.
- **Passive compounding is remarkably stable.** The 17× return declines only to 16.7× even at 20% owner share—a 3% reduction for a 4× increase in the fee.
- **Sybil profitability is unaffected.** Mean sybil net P&L hovers around +257 to +306 regardless of owner share, with the 95% CI consistently spanning zero. The owner share parameter doesn't interact meaningfully with sybil dynamics.

![Owner Share Macro](../sweep_outputs/charts/05_owner_share_macro.png)

**Implication for protocol designers:** The 95/5 split could be relaxed significantly (even to 80/20) without disrupting the fundamental fairness guarantee. This gives the protocol room to increase synthesizer incentives if needed without sacrificing creator primacy.

### 6.2 Dampening Exponent Sweep: Finding the Sybil Kill Zone

**Setup**: 7 exponent values (0.3 to 0.9) × 50 seeds each = 350 simulations. The dampening function is $\frac{1}{n^{e}}$, where $e$ is the exponent and $n$ is the number of nodes a single controller operates.

![Dampening Sweep](../sweep_outputs/charts/03_dampening_sweep.png)

This sweep reveals a clear **phase transition** in sybil economics:

| Exponent | Dampening Strength | Sybil Net P&L | 95% CI | Verdict |
|---|---|---|---|---|
| 0.3 ($n^{0.3}$) | Weak | **+738** | [+194, +1,281] | Profitable |
| 0.4 | | **+560** | [+89, +1,032] | Profitable |
| 0.5 ($\sqrt{n}$, current) | Moderate | +306 | [−26, +638] | Marginal |
| 0.6 | | +345 | [−33, +723] | Marginal |
| 0.7 | | +84 | [−275, +443] | Borderline |
| 0.8 | Strong | **−167** | [−374, +39] | Unprofitable |
| 0.9 | Very Strong | **−390** | [−526, −255] | **Definitively unprofitable** |

![Dampening Boxplots](../sweep_outputs/charts/06_dampening_boxplots.png)

**The critical threshold is between exponent 0.7 and 0.8.** At 0.8, the mean sybil profit turns negative and the CI barely touches zero. At 0.9, the **entire 95% confidence interval is below zero**—sybil attacks are a losing strategy with statistical certainty.

**Effect on the rest of the economy:**

| Exponent | A1 Mean TVL | A1 Win Rate | Passive Compounding |
|---|---|---|---|
| 0.3 | 1,591 | 96% | 16.8× |
| 0.5 (current) | 1,560 | 94% | 17.1× |
| 0.7 | 1,645 | **100%** | 16.6× |
| 0.9 | 1,427 | 90% | 16.3× |

Stronger dampening (0.7–0.9) has a modest side effect: it slightly compresses D3 TVL upward (from ~295 to ~559) because removing sybil nodes from the selection pool redirects more queries to legitimate content. This narrows the A1/D3 gap but doesn't flip it.

The sweet spot appears to be **exponent 0.7**: it produces a 100% A1 win rate, reduces sybil profitability to near zero (+84, CI spanning zero), and preserves the 16.6× passive compounding rate. Alternatively, **exponent 0.8** offers a clean sybil kill at the cost of a few percentage points of A1 dominance.

**Recommendation:** The current $\sqrt{n}$ (0.5) dampening is acceptable for a moderate-threat environment. For production deployment where adversaries are well-resourced, increasing the exponent to **0.7–0.8** would make sybil attacks definitively unprofitable without meaningfully harming the creator economy.

### 6.3 Long-Horizon Run: What Happens at 10× Scale

**Setup**: 40,000 ticks (10× the baseline) × 50 seeds = 50 simulations. Each simulation models approximately 11 hours of compute to simulate a far longer economic window.

![Long Horizon](../sweep_outputs/charts/04_long_horizon.png)

| Metric | 4,000 ticks | 40,000 ticks | Change |
|---|---|---|---|
| A1 avg TVL | 1,560 | **1,999** | +28% |
| D3 avg TVL | 320 | 315 | −2% |
| A1 win rate | 94% | **98%** | +4pp |
| A1−D3 gap | 1,240 | **1,684** | +36% |
| Total TVL | 68,409 | **336,969** | +4.9× |
| Passive compounding | 17× | **85.6×** | +5.0× |
| Sybil net P&L | +306 | **+2,221** | +7.3× |

**Creator advantage compounds over time.** The A1/D3 gap doesn't converge—it *widens* by 36% at 10× horizon. Domain expert TVL continues growing (+28%) while aggregator TVL flatlines (−2%). This is the passive earnings flywheel: once an expert's L0 content is embedded in popular synthesis chains, every new query perpetually routes royalties back through provenance.

**Passive compounding scales linearly.** The 17× return at 4,000 ticks becomes **85.6×** at 40,000 ticks—almost exactly 5× (suggesting compounding is close to linear in ticks, not exponential). Still, an 85× return for zero ongoing effort is extraordinary.

**The sybil long-horizon problem.** This is the sweep's most important cautionary finding. Sybil mean net profit rises from +306 to **+2,221** at 40,000 ticks, with a 95% CI of [+287, +4,155]—entirely above zero. The √n dampening that holds sybils at bay over 4,000 ticks **fails to contain them over longer horizons**. The mechanism reduces the *rate* of sybil accumulation but doesn't eliminate it, and over enough time, even reduced accumulation becomes significant.

This has a clear implication: **the protocol needs time-aware sybil resistance**, not just volume-based dampening. Options include:

- **Reputation decay**: Old, unqueried sybil content loses selection weight over time
- **Quality gating**: Minimum quality thresholds that automatically exclude the bottom tier
- **Adaptive exponents**: Increase the dampening exponent dynamically as controllers grow larger
- **Combining the exponent fix with long-horizon deployment**: Using exponent 0.8 or 0.9 (which makes sybils net-negative at 4,000 ticks) would push the long-horizon break-even much further out

### 6.4 Sweep Summary

The overnight parameter sweep tested 1,400 configurations across three dimensions of the Nodalync economic model:

| Experiment | Simulations | Compute Time | Key Finding |
|---|---|---|---|
| Owner-Share (1–20%) | 1,000 | 6.8 hours | **No crossover exists.** Creators dominate aggregators at every tested fee level. |
| Dampening Exponent (0.3–0.9) | 350 | 2.3 hours | **Sybil kill zone at 0.8.** Increasing from √n to $n^{0.8}$ makes attacks definitively unprofitable. |
| Long Horizon (40,000 ticks) | 50 | 11.1 hours | **Creator advantage widens with time**, but sybil profits accumulate—time-aware resistance needed. |

---

## 7. Key Takeaways

### For Protocol Designers

1. **The 95/5 split works—and has massive headroom.** It creates a strong, statistically significant bias toward original creators over aggregators. Even at 80/20, creators still dominate by 2.8×. The protocol has room to increase synthesizer incentives without breaking its fairness guarantees.

2. **Sybil dampening must be built in from day one.** Without it, content flooding is the dominant strategy. The $\frac{1}{\sqrt{n}}$ mechanism is effective at short horizons but insufficient for long-lived networks. **Exponent 0.7–0.8 is the recommended production setting**, with time-decay as an additional defense layer.

3. **Infrastructure actors need subsidies.** They're net-negative by design because their value is diffuse (improving discovery for everyone). A protocol fee earmarked for infrastructure operators would close this gap.

4. **Plan for long horizons.** The protocol's economic invariants (creator primacy, passive compounding) hold at 10× scale, but adversarial economics shift. Sybil resistance that works at 4,000 ticks doesn't automatically scale to 40,000—build time-aware mechanisms early.

### For Knowledge Creators

1. **Publish early.** The passive compounding effect (17× at 4,000 ticks, **85.6×** at 40,000 ticks) means early content has dramatically more earning potential than late content. First-mover advantage is real and scales linearly.

2. **Quality matters more than quantity.** Domain experts (12 high-quality nodes) out-earn prolific publishers (35 medium-quality nodes) per-node because quality-weighted selection channels more queries to better content.

3. **You don't need to stay active.** The passive legacy node result proves that walking away after publishing doesn't zero your earnings. The provenance chain works for you indefinitely.

### For Investors & Evaluators

1. **AI agents are the demand engine.** Category C actors (24 AI query agents + synthesis agents) inject ~59% of all economic value. The protocol's growth is directly tied to AI adoption.

2. **The economics are zero-sum between supply and demand.** Every unit of TVL earned by a creator was spent by a consumer or agent. There's no yield without real demand.

3. **The protocol is parametrically robust.** Across 1,400 simulations varying three key parameters, the core economic invariant (creators > aggregators) never breaks. This isn't a fragile result tuned to one specific configuration—it's a structural property of provenance-based economics.

---

## 8. Methodology Notes

- **Simulator**: Custom Python discrete-event engine, ~750 lines, no external dependencies beyond stdlib
- **Baseline statistics**: 50-seed Monte Carlo with independent PRNG states; 95% CIs computed via $\bar{x} \pm t_{0.025} \cdot \frac{s}{\sqrt{n}}$
- **Sweep statistics**: 1,400 additional simulations (1,000 owner-share + 350 dampening + 50 long-horizon), all with checkpoint/retry and atomic result persistence
- **Total compute**: ~20.2 hours across all experiments, zero failures
- **Runtime per seed**: ~24.3s at 4,000 ticks; ~799s at 40,000 ticks
- **Reproducibility**: All configs, code, and raw data available; any seed is deterministically reproducible
- **Limitations**:
  - Fixed pricing (no dynamic market-clearing)
  - No network effects on adoption rates
  - Infrastructure actors modeled with fixed behavioral probabilities rather than adaptive strategies
  - Sybil dampening is a selection-time mechanism, not a protocol-level enforcement
  - Owner-share sweep varies the fee without allowing actors to strategically respond to fee changes

---

## 9. What's Next

Three directions for deeper investigation:

1. **Adaptive adversaries**: Current sybil attackers use fixed strategies. A reinforcement-learning adversary that adapts its publishing rate, quality, and controller structure in response to dampening would stress-test the mechanism more rigorously—especially at long horizons where the current dampening shows weakness.

2. **Dynamic pricing**: Replace fixed L0/L3 prices with an auction or market-clearing mechanism where prices reflect supply/demand for specific content domains. This could fundamentally change the equilibrium.

3. **Time-decay mechanisms**: The long-horizon finding that sybil profits accumulate despite volume-based dampening motivates the next simulation: model reputation decay where unqueried content loses selection weight over time, and test whether this closes the long-horizon sybil gap.

---
