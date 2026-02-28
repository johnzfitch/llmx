---
chunk_index: 242
ref: "ae7a18cb2b7e"
id: "ae7a18cb2b7e874cc31a60f843a40de34ceabac74adc16aa7b32467729e444ee"
slug: "llmcat-integration-plan--1-1-minimal-python-implementation"
path: "/home/zack/dev/llmx/docs/LLMCAT_INTEGRATION_PLAN.md"
kind: "markdown"
lines: [61, 230]
token_estimate: 1528
content_sha256: "36e6b4134ae0a3d1c4a392274fc1292c901cd82be975cedd2a781d059f0e3d39"
compacted: false
heading_path: ["specho-v2 Integration for llm.cat: Token Efficiency Implementation","Phase 1: Core Analysis Engine","1.1: Minimal Python Implementation"]
symbol: null
address: null
asset_path: null
---

### 1.1: Minimal Python Implementation

**File**: `backend/quality_filter.py`

```python
import numpy as np
import re
from typing import List, Dict, Tuple

class ChunkQualityFilter:
    """
    Fast chunk quality analysis for token-efficient exports.
    Based on specho-v2 Layer E (6D features).
    """
    
    def __init__(self, min_quality: float = 0.6):
        self.min_quality = min_quality
        # Feature weights (tuned for code/docs)
        self.weights = np.array([0.15, 0.25, 0.20, 0.10, 0.20, 0.10])
    
    def filter_chunks(self, chunks: List[Dict]) -> Tuple[List[Dict], Dict]:
        """
        Filter chunks by quality, return filtered chunks + stats.
        
        Args:
            chunks: List of {id, content, path, lines}
        
        Returns:
            (filtered_chunks, stats)
            
        Stats:
            {
                'original_count': int,
                'filtered_count': int,
                'original_tokens': int,
                'filtered_tokens': int,
                'token_savings_pct': float
            }
        """
        scored_chunks = []
        original_tokens = 0
        
        for chunk in chunks:
            score = self._calculate_quality(chunk['content'])
            chunk['quality_score'] = score
            scored_chunks.append(chunk)
            original_tokens += self._estimate_tokens(chunk['content'])
        
        # Filter by minimum quality
        filtered = [c for c in scored_chunks if c['quality_score'] >= self.min_quality]
        
        # Sort by quality (best first)
        filtered.sort(key=lambda c: c['quality_score'], reverse=True)
        
        filtered_tokens = sum(self._estimate_tokens(c['content']) for c in filtered)
        
        stats = {
            'original_count': len(chunks),
            'filtered_count': len(filtered),
            'removed_count': len(chunks) - len(filtered),
            'original_tokens': original_tokens,
            'filtered_tokens': filtered_tokens,
            'token_savings': original_tokens - filtered_tokens,
            'token_savings_pct': ((original_tokens - filtered_tokens) / original_tokens * 100) if original_tokens > 0 else 0
        }
        
        return filtered, stats
    
    def _calculate_quality(self, text: str) -> float:
        """Calculate 6D quality score."""
        features = np.array([
            self._perplexity_proxy(text),
            self._burstiness(text),
            self._lexical_diversity(text),
            self._stopword_ratio(text),
            self._sentence_complexity(text),
            self._punctuation_density(text)
        ])
        
        # Normalize to 0-1
        normalized = self._normalize_features(features)
        
        # Weighted score
        score = float(np.dot(normalized, self.weights))
        return min(1.0, max(0.0, score))
    
    def _perplexity_proxy(self, text: str) -> float:
        """Unique bigrams / total bigrams (higher = less predictable)."""
        words = text.lower().split()
        if len(words) < 3:
            return 0.5
        bigrams = [f"{words[i]}_{words[i+1]}" for i in range(len(words)-1)]
        return len(set(bigrams)) / len(bigrams) if bigrams else 0.5
    
    def _burstiness(self, text: str) -> float:
        """Sentence length variance (higher = more varied)."""
        sentences = re.split(r'[.!?]+', text)
        lengths = [len(s.split()) for s in sentences if s.strip()]
        if len(lengths) < 2:
            return 5.0
        return float(np.std(lengths))
    
    def _lexical_diversity(self, text: str) -> float:
        """Unique words / total words (higher = richer vocab)."""
        words = re.findall(r'\b\w+\b', text.lower())
        if not words:
            return 0.0
        return len(set(words)) / len(words)
    
    def _stopword_ratio(self, text: str) -> float:
        """Function words / total words (lower = denser content)."""
        stopwords = {'the','a','an','and','or','but','in','on','at','to','for',
                    'of','with','by','from','as','is','was','are','be','this','that'}
        words = re.findall(r'\b\w+\b', text.lower())
        if not words:
            return 0.5
        return sum(1 for w in words if w in stopwords) / len(words)
    
    def _sentence_complexity(self, text: str) -> float:
        """Words per sentence (moderate is good)."""
        sentences = [s for s in re.split(r'[.!?]+', text) if s.strip()]
        if not sentences:
            return 10.0
        words = len(re.findall(r'\b\w+\b', text))
        return words / len(sentences)
    
    def _punctuation_density(self, text: str) -> float:
        """Punctuation per 100 words."""
        words = re.findall(r'\b\w+\b', text)
        if not words:
            return 0.0
        punct = len(re.findall(r'[.,;:!?()"\'-]', text))
        return (punct / len(words)) * 100
    
    def _normalize_features(self, features: np.ndarray) -> np.ndarray:
        """Normalize to 0-1 using expected ranges."""
        ranges = {
            0: (0.0, 1.0),    # perplexity
            1: (0.0, 20.0),   # burstiness
            2: (0.0, 1.0),    # lexical_diversity
            3: (0.0, 0.5),    # stopword_ratio (inverted: lower is better)
            4: (5.0, 50.0),   # complexity
            5: (0.0, 30.0)    # punctuation
        }
        
        normalized = np.zeros_like(features)
        for i, val in enumerate(features):
            min_val, max_val = ranges[i]
            norm = (val - min_val) / (max_val - min_val)
            # Invert stopword ratio (lower is better)
            if i == 3:
                norm = 1.0 - norm
            normalized[i] = min(1.0, max(0.0, norm))
        
        return normalized
    
    def _estimate_tokens(self, text: str) -> int:
        """Rough token estimate (chars / 4)."""
        return len(text) // 4

# Usage
filter = ChunkQualityFilter(min_quality=0.6)
filtered_chunks, stats = filter.filter_chunks(all_chunks)

print(f"Token savings: {stats['token_savings_pct']:.1f}%")
print(f"Removed {stats['removed_count']} low-quality chunks")
```

---