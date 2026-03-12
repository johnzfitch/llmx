function increment(map, key, amount = 1) {
  map.set(key, (map.get(key) || 0) + amount);
}

function toPlainObject(map) {
  return Object.fromEntries(map.entries());
}

function snippet(text, maxChars = 200) {
  if (!text) return "";
  const cleaned = String(text).replace(/\s+/g, " ").trim();
  return cleaned.length > maxChars ? `${cleaned.slice(0, maxChars - 3)}...` : cleaned;
}

function chunkKindLabel(kind) {
  switch (String(kind || "unknown")) {
    case "markdown":
      return "markdown";
    case "json":
      return "json";
    case "javascript":
      return "javascript";
    case "html":
      return "html";
    case "text":
      return "text";
    case "image":
      return "image";
    default:
      return "unknown";
  }
}

function astKindLabel(kind) {
  const value = String(kind || "other");
  return value || "other";
}

function edgeKindLabel(kind) {
  switch (String(kind || "")) {
    case "imports":
      return "imports";
    case "calls":
      return "calls";
    case "type_ref":
      return "type_ref";
    default:
      return "unknown";
  }
}

function fileExtensionLabel(path) {
  const value = String(path || "");
  const lastSlash = value.lastIndexOf("/");
  const file = lastSlash >= 0 ? value.slice(lastSlash + 1) : value;
  const lastDot = file.lastIndexOf(".");
  if (lastDot <= 0 || lastDot === file.length - 1) {
    return "(none)";
  }
  return file.slice(lastDot + 1).toLowerCase();
}

function normalizeSymbolKey(value) {
  return String(value || "").trim().toLowerCase();
}

function canonicalSymbolKey(value) {
  return normalizeSymbolKey(value);
}

function rawSymbolKey(value) {
  return normalizeSymbolKey(value);
}

function looksQualifiedSymbol(symbol) {
  return String(symbol || "").includes("::") || String(symbol || "").includes(".");
}

function matchPattern(name, pattern) {
  if (!pattern) return true;
  const value = String(pattern).trim().toLowerCase();
  if (!value) return true;
  const target = String(name || "").toLowerCase();
  if (value.startsWith("*") && value.endsWith("*")) {
    return target.includes(value.slice(1, -1));
  }
  if (value.endsWith("*")) {
    return target.startsWith(value.slice(0, -1));
  }
  if (value.startsWith("*")) {
    return target.endsWith(value.slice(1));
  }
  return target === value;
}

function buildChunkMap(index) {
  return new Map((index.chunks || []).map((chunk) => [chunk.id, chunk]));
}

function buildSymbolEntries(index) {
  const symbolTable = index.symbols || {};
  const entries = [];
  for (const values of Object.values(symbolTable)) {
    for (const entry of values || []) {
      entries.push(entry);
    }
  }
  return entries;
}

function sortSymbolEntries(entries) {
  entries.sort((a, b) => {
    return String(a.qualified_name || "")
      .localeCompare(String(b.qualified_name || ""))
      || String(a.path || "").localeCompare(String(b.path || ""))
      || Number(a.start_line || 0) - Number(b.start_line || 0);
  });
}

export function buildManageStats(index) {
  const fileKindBreakdown = new Map();
  const extensionBreakdown = new Map();
  const astKindBreakdown = new Map();
  const edgeKindBreakdown = new Map();
  const uniqueLanguages = new Set();
  const files = index.files || [];
  const chunks = index.chunks || [];
  const forwardEdges = (index.edges && index.edges.forward) || {};

  for (const file of files) {
    const kind = chunkKindLabel(file.kind);
    increment(fileKindBreakdown, kind);
    uniqueLanguages.add(kind);
    increment(extensionBreakdown, fileExtensionLabel(file.path));
  }

  for (const chunk of chunks) {
    if (chunk.ast_kind) {
      increment(astKindBreakdown, astKindLabel(chunk.ast_kind));
    }
  }

  for (const edges of Object.values(forwardEdges)) {
    for (const edge of edges || []) {
      increment(edgeKindBreakdown, edgeKindLabel(edge.edge_kind));
    }
  }

  const symbolEntries = buildSymbolEntries(index);
  const edgeCount = Object.values(forwardEdges).reduce((total, edges) => total + (edges ? edges.length : 0), 0);

  return {
    total_files: index.stats?.total_files ?? files.length,
    total_chunks: index.stats?.total_chunks ?? chunks.length,
    avg_chunk_tokens: index.stats?.avg_chunk_tokens ?? 0,
    symbol_count: symbolEntries.length,
    edge_count: edgeCount,
    language_count: uniqueLanguages.size,
    file_kind_breakdown: toPlainObject(fileKindBreakdown),
    extension_breakdown: toPlainObject(extensionBreakdown),
    ast_kind_breakdown: toPlainObject(astKindBreakdown),
    edge_kind_breakdown: toPlainObject(edgeKindBreakdown),
  };
}

export function listRichSymbols(index, input = {}) {
  const pattern = input.pattern || null;
  const astKind = input.ast_kind || null;
  const pathPrefix = input.path_prefix || null;
  const limit = Math.min(Number(input.limit ?? 50), 500);
  const entries = [];

  for (const chunk of index.chunks || []) {
    if (!chunk.ast_kind) continue;
    if (pathPrefix && !String(chunk.path || "").startsWith(pathPrefix)) continue;
    if (astKind && String(chunk.ast_kind) !== String(astKind)) continue;
    const qualifiedName = chunk.qualified_name || chunk.symbol || chunk.short_id || chunk.id;
    if (!matchPattern(qualifiedName, pattern)) continue;
    entries.push({
      qualified_name: qualifiedName,
      ast_kind: astKindLabel(chunk.ast_kind),
      path: chunk.path,
      start_line: chunk.start_line,
      end_line: chunk.end_line,
      signature: chunk.signature || null,
      doc_summary: chunk.doc_summary || null,
      exported: Array.isArray(chunk.exports) && chunk.exports.length > 0,
      chunk_id: chunk.id,
    });
  }

  sortSymbolEntries(entries);
  const total = entries.length;
  return {
    symbols: entries.slice(0, limit),
    total,
  };
}

export function lookupSymbols(index, input = {}) {
  const symbol = String(input.symbol || "").trim();
  const kind = input.kind ? String(input.kind).toLowerCase() : null;
  const pathPrefix = input.path_prefix || null;
  const limit = Math.min(Number(input.limit ?? 20), 200);
  const chunkMap = buildChunkMap(index);
  const symbolTable = index.symbols || {};
  const entries = [];
  const seen = new Set();
  const prefix = symbol.endsWith("*") ? normalizeSymbolKey(symbol.slice(0, -1)) : null;

  const push = (entry) => {
    if (!entry || seen.has(entry.chunk_id)) return;
    if (pathPrefix && !String(entry.path || "").startsWith(pathPrefix)) return;
    if (kind && String(entry.ast_kind || "").toLowerCase() !== kind) return;
    seen.add(entry.chunk_id);
    const chunk = chunkMap.get(entry.chunk_id);
    entries.push({
      qualified_name: entry.qualified_name,
      ast_kind: String(entry.ast_kind || "other"),
      path: entry.path,
      start_line: entry.start_line,
      end_line: entry.end_line,
      signature: entry.signature || null,
      doc_summary: entry.doc_summary || null,
      exported: Boolean(chunk && Array.isArray(chunk.exports) && chunk.exports.length),
      chunk_id: entry.chunk_id,
    });
  };

  if (!symbol) {
    return { matches: [], total: 0 };
  }

  if (prefix !== null) {
    for (const values of Object.values(symbolTable)) {
      for (const entry of values || []) {
        const name = normalizeSymbolKey(entry.name || "");
        const qualifiedName = normalizeSymbolKey(entry.qualified_name || "");
        if (name.startsWith(prefix) || qualifiedName.startsWith(prefix)) {
          push(entry);
        }
      }
    }
  } else {
    for (const entry of symbolTable[normalizeSymbolKey(symbol)] || []) {
      push(entry);
    }
  }

  sortSymbolEntries(entries);
  const total = entries.length;
  return {
    matches: entries.slice(0, limit),
    total,
  };
}

function resolveSymbolLookupKeys(index, symbol) {
  const normalized = normalizeSymbolKey(symbol);
  const symbolTable = index.symbols || {};
  const entries = symbolTable[normalized];
  if (!entries || !entries.length) {
    return [];
  }
  if (looksQualifiedSymbol(symbol)) {
    return [normalized];
  }

  const seen = new Set();
  const ordered = [];
  for (const entry of entries) {
    const key = canonicalSymbolKey(entry.qualified_name);
    if (!seen.has(key)) {
      seen.add(key);
      ordered.push(key);
    }
  }
  return ordered;
}

function resolveReverseKeys(index, symbol) {
  const keys = resolveSymbolLookupKeys(index, symbol);
  return keys.length ? keys : [rawSymbolKey(symbol)];
}

function lookupSymbolChunkIds(index, symbol) {
  const seen = new Set();
  const chunkIds = [];
  const symbolTable = index.symbols || {};
  for (const key of resolveSymbolLookupKeys(index, symbol)) {
    for (const entry of symbolTable[key] || []) {
      if (!seen.has(entry.chunk_id)) {
        seen.add(entry.chunk_id);
        chunkIds.push(entry.chunk_id);
      }
    }
  }
  return chunkIds;
}

function buildRefResult(edge, chunkMap, useTargetContext) {
  const sourceChunk = chunkMap.get(edge.source_chunk_id);
  if (!sourceChunk) return null;
  const targetChunk = edge.target_chunk_id ? chunkMap.get(edge.target_chunk_id) : null;
  const contextChunk = useTargetContext ? (targetChunk || sourceChunk) : sourceChunk;
  const targetSymbol = targetChunk
    ? (targetChunk.qualified_name || targetChunk.symbol || edge.target_symbol)
    : edge.target_symbol;
  return {
    source_symbol: sourceChunk.qualified_name || sourceChunk.symbol || sourceChunk.short_id,
    target_symbol: targetSymbol,
    path: contextChunk.path,
    start_line: contextChunk.start_line,
    end_line: contextChunk.end_line,
    ast_kind: contextChunk.ast_kind || null,
    signature: contextChunk.signature || null,
    context: snippet(contextChunk.content, 200),
    chunk_id: contextChunk.id,
    target_chunk_id: edge.target_chunk_id || null,
  };
}

function sortRefs(entries) {
  entries.sort((a, b) => {
    return String(a.path || "").localeCompare(String(b.path || ""))
      || Number(a.start_line || 0) - Number(b.start_line || 0)
      || String(a.source_symbol || "").localeCompare(String(b.source_symbol || ""))
      || String(a.target_symbol || "").localeCompare(String(b.target_symbol || ""));
  });
}

export function traceRefs(index, input = {}) {
  const symbol = String(input.symbol || "").trim();
  const direction = String(input.direction || "").toLowerCase();
  const depth = Math.max(1, Math.min(Number(input.depth ?? 1), 8));
  const limit = Math.min(Number(input.limit ?? 20), 200);
  const chunkMap = buildChunkMap(index);
  const forward = (index.edges && index.edges.forward) || {};
  const reverse = (index.edges && index.edges.reverse) || {};
  const results = [];
  const seenRefs = new Set();

  if (!symbol) {
    return { refs: [], total: 0 };
  }

  if (direction === "callers" || direction === "importers" || direction === "type_users") {
    const edgeKind = direction === "callers" ? "calls" : direction === "importers" ? "imports" : "type_ref";
    let frontier = resolveReverseKeys(index, symbol);
    const visitedSymbols = new Set();

    for (let level = 0; level < depth; level += 1) {
      const nextFrontier = [];
      for (const key of frontier) {
        if (visitedSymbols.has(key)) continue;
        visitedSymbols.add(key);
        for (const edge of reverse[key] || []) {
          if (edge.edge_kind !== edgeKind) continue;
          const seenKey = `${edge.source_chunk_id}:${edge.target_symbol}:${edge.edge_kind}`;
          if (seenRefs.has(seenKey)) continue;
          seenRefs.add(seenKey);
          const ref = buildRefResult(edge, chunkMap, false);
          if (ref && results.length < limit) {
            results.push(ref);
          }
          const sourceChunk = chunkMap.get(edge.source_chunk_id);
          const sourceSymbol = sourceChunk && (sourceChunk.qualified_name || sourceChunk.symbol);
          if (sourceSymbol) {
            nextFrontier.push(canonicalSymbolKey(sourceSymbol));
          }
        }
      }
      if (!nextFrontier.length || results.length >= limit) break;
      frontier = nextFrontier;
    }
  } else if (direction === "callees" || direction === "imports") {
    const edgeKind = direction === "callees" ? "calls" : "imports";
    let frontier = lookupSymbolChunkIds(index, symbol);
    const visitedChunks = new Set();

    for (let level = 0; level < depth; level += 1) {
      const nextFrontier = [];
      for (const chunkId of frontier) {
        if (visitedChunks.has(chunkId)) continue;
        visitedChunks.add(chunkId);
        for (const edge of forward[chunkId] || []) {
          if (edge.edge_kind !== edgeKind) continue;
          const seenKey = `${edge.source_chunk_id}:${edge.target_symbol}:${edge.edge_kind}`;
          if (seenRefs.has(seenKey)) continue;
          seenRefs.add(seenKey);
          const ref = buildRefResult(edge, chunkMap, true);
          if (ref && results.length < limit) {
            results.push(ref);
          }
          if (edge.target_chunk_id) {
            nextFrontier.push(edge.target_chunk_id);
          } else {
            nextFrontier.push(...lookupSymbolChunkIds(index, edge.target_symbol));
          }
        }
      }
      if (!nextFrontier.length || results.length >= limit) break;
      frontier = nextFrontier;
    }
  } else {
    throw new Error(`Invalid direction: ${direction}. Use callers, callees, importers, imports, or type_users.`);
  }

  sortRefs(results);
  return {
    refs: results,
    total: results.length,
  };
}

function normalizeScore(results, key = "score") {
  let max = 0;
  for (const result of results) {
    max = Math.max(max, Number(result[key] || 0));
  }
  if (max <= 0) {
    return new Map(results.map((result) => [result.chunk_id, 0]));
  }
  return new Map(results.map((result) => [result.chunk_id, Number(result[key] || 0) / max]));
}

export function rrfFuse(bm25Results, semanticResults, limit) {
  const k = 60;
  const scores = new Map();

  const addResults = (results) => {
    results.forEach((result, rank) => {
      scores.set(result.chunk_id, (scores.get(result.chunk_id) || 0) + 1 / (k + rank + 1));
    });
  };

  addResults(bm25Results);
  addResults(semanticResults);

  return Array.from(scores.entries())
    .map(([chunkId, score]) => ({ chunkId, score }))
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);
}

export function linearFuse(bm25Results, semanticResults, limit) {
  const lexical = normalizeScore(bm25Results);
  const dense = normalizeScore(semanticResults);
  const ids = new Set([...lexical.keys(), ...dense.keys()]);
  return Array.from(ids)
    .map((chunkId) => ({
      chunkId,
      score: (lexical.get(chunkId) || 0) * 0.4 + (dense.get(chunkId) || 0) * 0.6,
    }))
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);
}
