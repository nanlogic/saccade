# Saccade Extension 0.4.3

This release hardens ordinary activation and continuous observation on
mutation-heavy web applications.

- Native links, buttons, inputs, and summaries are activated on the exact
  authoritative element instead of retargeting the click to a framework child.
- Custom ARIA wrappers can still delegate to a current native control in an
  open shadow root.
- Mutation- and geometry-driven Truth compilation is coalesced with a bounded
  main-thread interval on ordinary pages. Dedicated continuous reflex targets
  retain frame-rate tracking.
- Chrome and Edge release gates now prove both custom-control delegation and
  direct native-link activation.

No selectors, DOM paths, arbitrary JavaScript, or arbitrary-coordinate action
surface is exposed to the Agent.
