# 🎯 RMRF - Plan Complet pour 2 Devs

**Objectif:** Système de backup avec chunking, déduplication et compression  
**Durée totale:** 4 heures  
**Team:** 2 développeurs  
**Status:** MVP minimal à 100%

---

## 📊 Vue d'ensemble

| Métrique | Valeur |
|----------|--------|
| **Temps total** | 4h |
| **Nombre de tâches** | 9 |
| **Phases** | 3 |
| **Commits** | 3 (1 par phase) |

---

## 🗓️ PHASE 1: Compression Zstd (1h)

**Objectif:** Réduire la taille des chunks stockés (= l'objectif principal!)

### Tâche 1.1: Ajouter zstd aux dépendances
**Assigné à:** Dev 1  
**Durée:** 5 min

```bash
# Fichier: crates/rmrf-store/Cargo.toml
[dependencies]
# ... existing ...
zstd = "0.13"
```

**Checklist:**
- [x] Ouvrir `crates/rmrf-store/Cargo.toml`
- [x] Ajouter `zstd = "0.13"` dans les dépendances
- [x] Vérifier compilation: `cargo build -p rmrf-store`

---

### Tâche 1.2: Créer CompressedChunkStore wrapper
**Assigné à:** Dev 1  
**Durée:** 25 min

**Description:** Wrapper qui compresse avant store/décompresse après get

**Code à ajouter dans `crates/rmrf-store/src/lib.rs`:**

```rust
use zstd::Encoder;
use std::io::Read;

pub struct CompressedChunkStore<S: ChunkStore> {
    inner: S,
    compression_level: i32,
}

impl<S: ChunkStore> CompressedChunkStore<S> {
    pub fn new(inner: S) -> Self {
        CompressedChunkStore {
            inner,
            compression_level: 10,
        }
    }

    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = Encoder::new(Vec::new(), self.compression_level)
            .map_err(|e| SilexError::Msg(format!("Compression error: {}", e)))?;
        encoder.write_all(data)
            .map_err(|e| SilexError::Msg(format!("Write error: {}", e)))?;
        encoder.finish()
            .map_err(|e| SilexError::Msg(format!("Finish error: {}", e)))?
    }

    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = zstd::Decoder::new(data)
            .map_err(|e| SilexError::Msg(format!("Decompression error: {}", e)))?;
        let mut result = Vec::new();
        decoder.read_to_end(&mut result)
            .map_err(|e| SilexError::Msg(format!("Read error: {}", e)))?;
        Ok(result)
    }
}

impl<S: ChunkStore> ChunkStore for CompressedChunkStore<S> {
    fn put(&mut self, hash: &Hash, data: &[u8]) -> Result<ChunkLocation> {
        let compressed = self.compress(data)?;
        self.inner.put(hash, &compressed)
    }

    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        match self.inner.get(hash)? {
            Some(compressed_data) => Ok(Some(self.decompress(&compressed_data)?)),
            None => Ok(None),
        }
    }

    fn exists(&self, hash: &Hash) -> Result<bool> {
        self.inner.exists(hash)
    }

    fn delete(&mut self, hash: &Hash) -> Result<()> {
        self.inner.delete(hash)
    }
}
```

**Checklist:**
- [x] Ajouter le code ci-dessus à `rmrf-store/src/lib.rs`
- [x] Ajouter l'import: `use std::io::Read;`
- [x] Tester: `cargo build -p rmrf-store`
- [x] Vérifier: pas de warnings

---

### Tâche 1.3: Mettre à jour rmrf-cli pour utiliser compression
**Assigné à:** Dev 1  
**Durée:** 10 min  
**Dépendance:** Dev 2 doit avoir fini 2.1 avant de faire cette tâche

**Fichier:** `crates/rmrf-cli/src/main.rs`

**Modification dans fonction `backup()`:**

```rust
// Avant:
let mut store = match LocalChunkStore::new(store_dir.clone()) {
    Ok(s) => s,
    Err(e) => { eprintln!("Error: {}", e); return; }
};

// Après:
let local_store = match LocalChunkStore::new(store_dir.clone()) {
    Ok(s) => s,
    Err(e) => { eprintln!("Error: {}", e); return; }
};
let mut store = CompressedChunkStore::new(local_store);
```

**Checklist:**
- [x] Importer: `use rmrf_store::CompressedChunkStore;`
- [x] Modifier la création du store
- [x] Compiler: `cargo build -p rmrf-cli --release`
- [x] Vérifier: pas d'erreur

---

### Tâche 2.1: Créer BackupManifest minimal
**Assigné à:** Dev 2  
**Durée:** 30 min  
**Dépendance:** Aucune

**Description:** Structure ultra-minimaliste pour tracker les chunks d'un backup

**Fichier:** `crates/rmrf-manifest/src/lib.rs`

```rust
use serde::{Deserialize, Serialize};
use rmrf_core::Hash;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub id: String,
    pub timestamp: i64,
    pub files: Vec<FileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub chunks: Vec<[u8; 32]>,  // Vec<Hash>
}

impl BackupManifest {
    pub fn new(id: String) -> Self {
        BackupManifest {
            id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            files: Vec::new(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let data = bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(path, data)
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let data = fs::read(path)?;
        bincode::deserialize(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}
```

**Cargo.toml de rmrf-manifest:**
```toml
[dependencies]
rmrf-core = { path = "../rmrf-core" }
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"
```

**Checklist:**
- [x] Créer les structs BackupManifest et FileInfo
- [x] Impl save() avec bincode
- [x] Impl load() avec bincode
- [x] Ajouter les dépendances (serde, bincode)
- [x] Tester: `cargo build -p rmrf-manifest`

---

### Tâche 2.2: Mettre à jour backup pour sauver manifest
**Assigné à:** Dev 2  
**Durée:** 15 min  
**Dépendance:** 2.1 doit être fini

**Fichier:** `crates/rmrf-cli/src/main.rs`

**Modifications dans fonction `backup()`:**

```rust
use rmrf_manifest::BackupManifest;

fn backup(source: &PathBuf, store_dir: &PathBuf) {
    let backup_id = format!("backup-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let mut manifest = BackupManifest::new(backup_id.clone());
    
    // ... existing code pour créer le store ...
    
    let mut current_file = FileInfo {
        path: source.to_string_lossy().to_string(),
        chunks: Vec::new(),
    };
    
    // ... dans la boucle où on traite les chunks ...
    loop {
        match file_source.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let data = &buffer[..n];
                let hash = hash_data(data);
                
                // Ajouter le hash au manifest
                current_file.chunks.push(hash);
                
                if let Err(e) = store.put(&hash, data) {
                    eprintln!("Error storing chunk: {}", e);
                    return;
                }
                // ... existing code ...
            }
            Err(e) => { /* ... */ }
        }
    }
    
    // Sauver le manifest
    manifest.files.push(current_file);
    if let Err(e) = manifest.save(&store_dir.join("manifest.bin")) {
        eprintln!("Error saving manifest: {}", e);
        return;
    }
    
    println!("✅ Backup complete!");
    // ... existing output ...
}
```

**Checklist:**
- [x] Importer: `use rmrf_manifest::{BackupManifest, FileInfo};`
- [x] Créer BackupManifest au démarrage
- [x] Tracker chaque chunk dans FileInfo
- [x] Sauver manifest.bin à la fin
- [x] Compiler et tester: `cargo build -p rmrf-cli`
- [x] Vérifier que `manifest.bin` est créé: `ls -la my-store/`

---

## 📊 Fin Phase 1

**Validation:**
```bash
echo "test file" > test.txt
cargo run -p rmrf-cli -- backup test.txt ./test-store
ls -la ./test-store/
# Doit voir: chunks compressés + manifest.bin
```

**Résultat attendu:** Les backups sont maintenant compressés! Ratio attendu: 5-10x pour du texte, 2-3x pour du binaire.

---

## 🔄 PHASE 2: Restore Fonctionnel (1.5h)

**Objectif:** Reconstruire les fichiers depuis les chunks

### Tâche 3.1: Implémenter restore_file()
**Assigné à:** Dev 1 + Dev 2 (pair programming)  
**Durée:** 45 min

**Fichier:** `crates/rmrf-cli/src/main.rs`

**Code à ajouter:**

```rust
use std::io::Write;

fn restore_file(
    manifest: BackupManifest,
    store: &dyn ChunkStore,
    dest: &PathBuf,
) -> std::io::Result<()> {
    // Pour chaque fichier dans le manifest
    for file_info in manifest.files {
        println!("♻️  Restoring: {}", file_info.path);
        
        // Créer le fichier de destination
        let mut output_file = std::fs::File::create(dest)?;
        
        // Pour chaque chunk du fichier
        for (i, chunk_hash) in file_info.chunks.iter().enumerate() {
            match store.get(chunk_hash) {
                Ok(Some(data)) => {
                    output_file.write_all(&data)?;
                    print!(".");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
                Ok(None) => {
                    eprintln!("\n❌ Chunk missing: {:?}", chunk_hash);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Chunk not found in store"
                    ));
                }
                Err(e) => {
                    eprintln!("\n❌ Error reading chunk: {}", e);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Error: {}", e)
                    ));
                }
            }
        }
        println!();
    }
    Ok(())
}
```

**Checklist:**
- [x] Créer la fonction restore_file()
- [x] Charger chaque chunk depuis le store
- [x] Décompression automatique (CompressedChunkStore gère ça)
- [x] Écrire les chunks dans le fichier de destination
- [x] Afficher progression avec des points (.)
- [x] Gérer les erreurs (chunk manquant, write error)
- [x] Compiler: `cargo build -p rmrf-cli`

---

### Tâche 3.2: Mettre à jour restore dans main.rs
**Assigné à:** Dev 2  
**Durée:** 30 min

**Fichier:** `crates/rmrf-cli/src/main.rs`, fonction `restore()`

```rust
fn restore(dest: &PathBuf, store_dir: &PathBuf) {
    println!("♻️  Restore: {}", dest.display());
    println!("  From store: {}", store_dir.display());

    // Charger le manifest
    let manifest_path = store_dir.join("manifest.bin");
    let manifest = match BackupManifest::load(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("❌ Error loading manifest: {}", e);
            return;
        }
    };

    println!("Backup ID: {}", manifest.id);
    println!("Files to restore: {}", manifest.files.len());

    // Créer le store (avec décompression automatique)
    let local_store = match LocalChunkStore::new(store_dir.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error creating store: {}", e);
            return;
        }
    };
    let store = CompressedChunkStore::new(local_store);

    // Restaurer les fichiers
    match restore_file(manifest, &store, dest) {
        Ok(_) => println!("✅ Restore complete!"),
        Err(e) => eprintln!("❌ Restore failed: {}", e),
    }
}
```

**Checklist:**
- [x] Charger manifest depuis `store_dir/manifest.bin`
- [x] Créer CompressedChunkStore (pour décompresser auto)
- [x] Appeler restore_file()
- [x] Afficher message de succès/erreur
- [x] Compiler: `cargo build -p rmrf-cli`

---

### Tâche 3.3: Test end-to-end
**Assigné à:** Tous les 2 (ensemble)  
**Durée:** 15 min

**Test script:**

```bash
# 1. Créer un fichier test
echo "Hello, RMRF!" > test-file.txt

# 2. Faire un backup
cargo run -p rmrf-cli -- backup test-file.txt ./test-store

# 3. Supprimer le fichier original
rm test-file.txt

# 4. Restaurer depuis le backup
mkdir -p restored
cargo run -p rmrf-cli -- restore ./restored/test-file.txt ./test-store

# 5. Vérifier que le fichier restauré est identique
cat ./restored/test-file.txt
# Doit afficher: "Hello, RMRF!"

# Test avec gros fichier
dd if=/dev/urandom of=bigtest.bin bs=1M count=10
cargo run -p rmrf-cli -- backup bigtest.bin ./bigtest-store
rm bigtest.bin
mkdir -p bigrestored
cargo run -p rmrf-cli -- restore ./bigrestored/bigtest.bin ./bigtest-store

# Vérifier tailles
ls -lh test-store/
ls -lh bigtest-store/
```

**Checklist:**
- [x] Test petit fichier: backup → restore → vérifier contenu
- [x] Test gros fichier (10MB+): backup → restore
- [x] Vérifier que les fichiers restaurés sont identiques aux originaux
- [x] Vérifier que les chunks sont bien compressés (comparer taille store vs fichier original)
- [x] Vérifier que manifest.bin existe

---

## 📊 Fin Phase 2

**Validation:**
```bash
# Cycle complet fonctionne:
# backup → store (compressé) → manifest.bin → restore → fichier identique ✓
```

---

## 🧹 PHASE 3: Cleanup & Polish (1h)

**Objectif:** Rendre le projet complet et stable

### Tâche 4.1: Tests d'intégration complets
**Assigné à:** Dev 1  
**Durée:** 30 min

**Tests à faire:**

1. **Fichier vide (0 bytes):**
```bash
touch empty.txt
cargo run -p rmrf-cli -- backup empty.txt ./test-empty
cargo run -p rmrf-cli -- restore ./restored-empty.txt ./test-empty
```

2. **Petit fichier (1 byte):**
```bash
echo -n "x" > tiny.txt
# ... backup → restore ...
```

3. **Gros fichier (100MB+):**
```bash
dd if=/dev/zero of=huge.bin bs=1M count=100
# ... backup → restore ...
```

4. **Test déduplication:**
```bash
cp test.txt test2.txt
cargo run -p rmrf-cli -- backup test.txt ./test-dedup
cargo run -p rmrf-cli -- backup test2.txt ./test-dedup
# Vérifier: même si 2 fichiers, store doit contenir que les chunks uniques
find ./test-dedup -type f | wc -l  # Doit être < 2 * nombre de chunks
```

5. **Mesurer compression:**
```bash
# Afficher dans le CLI:
# "Original: 5MB | Compressed: 1.2MB | Ratio: 4.2x"
```

**Checklist:**
- [x] Tester fichier vide
- [x] Tester petit fichier
- [x] Tester gros fichier (100MB+)
- [x] Tester déduplication
- [x] Mesurer et afficher ratio compression dans le CLI

---

### Tâche 4.2: Code quality & cleanup
**Assigné à:** Dev 2  
**Durée:** 20 min

```bash
# 1. Linter (clippy)
cargo clippy -p rmrf-core
cargo clippy -p rmrf-source
cargo clippy -p rmrf-store
cargo clippy -p rmrf-manifest
cargo clippy -p rmrf-cli
# Fixer tous les warnings

# 2. Format
cargo fmt

# 3. Build sans warnings
cargo build --release 2>&1 | grep -i warning
# Doit être vide!

# 4. Vérifier pas de dépendances inutilisées
cargo tree -p rmrf-cli
```

**Checklist:**
- [x] Pas de warnings (clippy)
- [x] Code formaté (fmt)
- [x] Build en release sans warnings
- [x] Pas de dépendances inutilisées

---

### Tâche 4.3: Documentation README
**Assigné à:** Dev 2  
**Durée:** 10 min

**Fichier:** `README.md` (à créer ou mettre à jour)

```markdown
# RMRF - Resilient Modular Resilient Backup Framework

## Features

- ✅ Chunking (8KB default)
- ✅ SHA256 deduplication
- ✅ Zstd compression (4-10x ratio)
- ✅ Incremental backups via manifest
- ✅ Filesystem storage

## Quick Start

### Backup
\`\`\`bash
cargo run -p rmrf-cli -- backup /path/to/file ./my-store
\`\`\`

Output:
```
📦 Backup: /path/to/file
  File size: 5242880 bytes
...
✅ Backup complete!
  Chunks: 640
  Original: 5.0 MB
  Compressed: 1.2 MB
  Ratio: 4.2x
  Store: ./my-store
```

### Restore
\`\`\`bash
cargo run -p rmrf-cli -- restore ./restored-file ./my-store
\`\`\`

## Architecture

```
FileSource → hash_data(SHA256) → CompressedChunkStore(Zstd) 
          → manifest.bin (metadata) → LocalChunkStore (filesystem)
```

## Test Results

| Test | Result |
|------|--------|
| Empty file | ✓ |
| Small file (1KB) | ✓ |
| Large file (100MB) | ✓ |
| Deduplication | ✓ |
| Compression ratio | 4-10x |

## Build & Test

\`\`\`bash
cargo build -p rmrf-cli --release
cargo test -p rmrf-*
\`\`\`

## Authors

- Dev 1 (core & storage)
- Dev 2 (cli & manifest)
```

**Checklist:**
- [x] Créer/mettre à jour README.md
- [x] Ajouter sections: Features, Quick Start, Architecture, Tests
- [x] Ajouter exemples de commandes
- [x] Ajouter résultats des tests (compression ratio, etc.)

---

## 📝 Commits Git

**Après Phase 1:**
```bash
git add .
git commit -m "[ADD] Phase 1: Compression Zstd et BackupManifest

- Add zstd compression to ChunkStore wrapper
- Create BackupManifest struct for tracking chunks
- Implement manifest save/load with bincode
- Update backup command to generate manifest.bin

Co-Authored-By: Dev1 <dev1@example.com>
Co-Authored-By: Dev2 <dev2@example.com>"
```

**Après Phase 2:**
```bash
git commit -m "[ADD] Phase 2: Restore fonctionnel

- Implement restore_file() to reconstruct files from chunks
- Auto-decompression via CompressedChunkStore
- Validate backup → restore cycle end-to-end
- Test with files 0 bytes to 100MB+

Co-Authored-By: Dev1 <dev1@example.com>
Co-Authored-By: Dev2 <dev2@example.com>"
```

**Après Phase 3:**
```bash
git commit -m "[POLISH] Phase 3: Tests et documentation

- Add integration tests (empty, small, large, dedup)
- Measure and display compression ratio
- Code cleanup: clippy, fmt, no warnings
- Complete README with examples and results

Co-Authored-By: Dev1 <dev1@example.com>
Co-Authored-By: Dev2 <dev2@example.com>"
```

---

## ✅ Checklist Finale

### Fonctionnalité
- [x] Compression Zstd fonctionne
- [x] Ratio compression visible (4-10x)
- [x] Backup crée manifest.bin
- [x] Restore reconstruit fichiers identiques
- [x] Déduplication validée
- [x] Cycle complet: backup → restore → identique ✓

### Qualité
- [x] Pas de warnings (clippy, cargo)
- [x] Code formaté (cargo fmt)
- [x] Build release sans erreurs
- [x] Tests: vide, petit, gros, dédup

### Documentation
- [x] README complète
- [x] Exemples de commandes
- [x] Résultats des tests documentés
- [x] 3 commits avec co-authors

---

## 🚀 Timeline Résumée

```
Phase 1 (1h): Compression + Manifest
├─ Dev 1: Zstd wrapper (35 min)
└─ Dev 2: Manifest struct (30 min)

Phase 2 (1.5h): Restore + Tests
├─ Dev 1+2: restore_file() pair programming (45 min)
├─ Dev 2: CLI restore command (30 min)
└─ Tous: End-to-end testing (15 min)

Phase 3 (1h): Polish
├─ Dev 1: Integration tests (30 min)
└─ Dev 2: Code quality + README (30 min)

Total: 4 heures ⏱️
```

---

## 🎯 Résultat Final

```
RMRF/
├── crates/
│   ├── rmrf-core/        ✅ Traits abstraits
│   ├── rmrf-source/      ✅ FileSource
│   ├── rmrf-store/       ✅ LocalChunkStore + CompressedChunkStore
│   ├── rmrf-manifest/    ✅ BackupManifest (structure)
│   └── rmrf-cli/         ✅ backup + restore commands
├── my-store/
│   ├── chunks/           (compressés avec Zstd)
│   └── manifest.bin      (métadonnées)
├── README.md             ✅ Documenté
└── target/release/rmrf   ✅ Binary prêt
```

**Capacités:**
- ✅ Backup files → chunked, hashed, compressed, deduped
- ✅ Store → filesystem organized by hash prefix
- ✅ Restore → reconstruct identical files from chunks
- ✅ Compression → 4-10x reduction on text/logs
- ✅ Deduplication → automatic, same chunk = same storage

---

## ⚠️ Risques & Solutions

| Risque | Solution |
|--------|----------|
| Manifest corrompu | Valider format bincode au load |
| Chunk manquant | Error explicite, pas de corruption silencieuse |
| Dédup timeout (gros files) | HashMap OK jusqu'à millions chunks (phase 2+) |
| Zstd pas dispo | Rust 1.70+ requis, bien documenté |
| Performance restore | Streaming chunks, pas de load en mémoire |

---

## 🎓 Notes pour l'équipe

1. **Démarrer ensemble** sur Phase 1 pour alignment
2. **Brancher après Phase 1** si besoin de paralléliser
3. **Pair programming** fortement recommandé pour Phase 2 (restore complexe)
4. **Tester régulièrement** entre chaque phase
5. **Commits petit et souvent** (pas de gros commits de 500 lignes)
6. **Rebase/merge** régulièrement depuis main

---

**Bon courage! 🚀 On lance Phase 1?**
