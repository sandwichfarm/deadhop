---
title: Markdown and llms.txt
description: Machine-readable versions of the deadhop site and docs. llms.txt, llms-full.txt, a Markdown twin of every page, a sitemap and structured data.
---

# Markdown and llms.txt

Everything on this site is available in plain text for agents and tools.

| Resource | What it is |
| --- | --- |
| <a href="../llms.txt" target="_self"><code>/llms.txt</code></a> | Index of the site in the [llms.txt](https://llmstxt.org) format |
| <a href="../llms-full.txt" target="_self"><code>/llms-full.txt</code></a> | The landing page and all docs pages in one Markdown file |
| <a href="../index.md" target="_self"><code>/index.md</code></a> | The landing page as Markdown |
| `/docs/<page>.md` | Every docs page as Markdown: replace `.html` with `.md` |
| <a href="../sitemap.xml" target="_self"><code>/sitemap.xml</code></a> | Every page, for crawlers |
| <a href="../robots.txt" target="_self"><code>/robots.txt</code></a> | Crawling rules; AI crawlers are allowed |

Each HTML page links its Markdown version with
`<link rel="alternate" type="text/markdown">` and carries schema.org structured data
(`SoftwareApplication` on the landing page, `TechArticle` on docs pages).

To use deadhop itself from an agent, see the [Tools API](./api.md): it is an MCP server, so
an agent with a ContextVM transport can call its tools directly.
