#共通ヘッダ
`size` は可変　stroke-widthはiconのサイズに比例させる
```
<svg version="1.1" width="`size`" height="`size`" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" xmlns="http://www.w3.org/2000/svg">
```
- folder
```

```

- zoom
```
  <circle cx="11" cy="11" r="7"/>
  <line x1="21" y1="21" x2="16.65" y2="16.65"/>
```

- zoom in
```
  <circle cx="11" cy="11" r="7"/>
  <line x1="11" y1="8" x2="11" y2="14"/>
  <line x1="8" y1="11" x2="14" y2="11"/>
  <line x1="21" y1="21" x2="16.65" y2="16.65"/>
```

- zoom out
```
  <circle cx="11" cy="11" r="7"/>
  <line x1="8" y1="11" x2="14" y2="11"/>
  <line x1="21" y1="21" x2="16.65" y2="16.65"/>
```

- sort
```
  <line x1="4" y1="7" x2="20" y2="6"/>
  <line x1="6" y1="12" x2="18" y2="12"/>
  <line x1="8" y1="17" x2="16" y2="18"/>
```

- \+asc
```
  <polyline points="6 15 12 9 18 15"/>
```

- \+desc
```
  <polyline points="6 9 12 15 18 9"/>
```

- sort_by_date
```
  <rect x="3" y="4" width="18" height="18" rx="2"/>
  <line x1="3" y1="10" x2="21" y2="10"/>
```

- sort_by_name
  AとZはtextではなくpathに置き換えてください
```
  <text x="4" y="16" font-size="10" fill="currentColor">A</text>
  <text x="12" y="16" font-size="10" fill="currentColor">Z</text>
```

- sort_by_size
```
  <rect x="4" y="10" width="4" height="10"/>
  <rect x="10" y="6" width="4" height="14"/>
  <rect x="16" y="2" width="4" height="18"/>
```

- filter
```
  <polygon points="3 4 21 4 14 12 14 20 10 18 10 12 3 4"/>
```

- filter_by_extension
```
  <line x1="8" y1="67 x2="20" y2="6"/>
  <line x1="8" y1="12" x2="20" y2="12"/>
  <line x1="8" y1="17" x2="20" y2="18"/>
  <circle cx="4" cy="6" r="1"/>
  <circle cx="4" cy="12" r="1"/>
  <circle cx="4" cy="18" r="1"/>
```

- detailed_list
```
  <rect x="3" y="5" width="18" height="14"/>
  <line x1="3" y1="10" x2="21" y2="10"/>
  <line x1="10" y1="5" x2="10" y2="19"/>
```

- thumbnail
```
  <rect x="3" y="3" width="7" height="7"/>
  <rect x="14" y="3" width="7" height="7"/>
  <rect x="3" y="14" width="7" height="7"/>
  <rect x="14" y="14" width="7" height="7"/>
```

size of thumnails
- large
```
  <rect x="3" y="3" width="18" height="18"/>
```

- middle
```
  <rect x="5" y="5" width="14" height="14"/>
```

- small
```
<rect x="7" y="7" width="10" height="10"/>
```

- folder
```
  <path d="M3 7h6l2 2h10v10a2 2 0 0 1-2 2H3z"/>
  <path d="M3 7V5a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v2"/>
```

- archive(`zip`などの拡張子で区別)
```
  <rect x="3" y="4" width="18" height="4" rx="1"/>
  <path d="M5 8v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8"/>
  <line x1="10" y1="12" x2="14" y2="12"/>
```

- image(サムネイルローディング前/省略時)`jpg`などの拡張子

```
  <!-- image枠 -->
  <rect x="3" y="3" width="18" height="14" rx="2"/>
  
  <!-- 山 -->
  <polyline points="5 14 9 10 13 14"/>
  
  <!-- 太陽 -->
  <circle cx="16" cy="7" r="1.5"/>

  <!-- extension風 -->
  <line x1="5" y1="19" x2="19" y2="19"/>
  <line x1="5" y1="21" x2="19" y2="21"/>

```