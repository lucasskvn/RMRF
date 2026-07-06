# Scripts de test RMRF

## Vue d'ensemble

| Script | Type | Nombre | Description |
|--------|------|--------|-------------|
| `test_basic.sh` | Intégration | 6 | Cycle backup→restore→diff sur fichiers variés |
| `test_dedup.sh` | Déduplication | 4 | Vérification que le backup idempotent ne duplique pas les chunks |
| `test_edge_cases.sh` | Cas limites | 8 | Fichiers vides, 1 octet, motifs binaires, Unicode |
| `test_sizes.sh` | Stress tailles | 9 | De 1 KB à 5 MB, données aléatoires |
| `test_errors.sh` | Gestion d'erreurs | 8 | Commandes invalides, fichiers manquants, corruption |
| `test_all.sh` | Runner global | — | Exécute tous les scripts ci-dessus + `cargo test` |

---

## `test_basic.sh` — Intégration de base

**Objectif** : Valider le cycle complet backup → restore → identité bit-à-bit pour les cas d'usage courants.

| # | Test | Contenu | Vérification |
|---|------|---------|-------------|
| 1 | Fichier texte simple | Chaîne ASCII | `diff` bit-à-bit |
| 2 | Fichier vide | 0 octet | Fichier restauré existe et est vide |
| 3 | 1 octet | Lettre "A" | `diff` bit-à-bit |
| 4 | Octets nuls (1 KB) | `/dev/zero` | `diff` bit-à-bit |
| 5 | Motif répété | "AB" × 5000 (~10 KB) | `diff` bit-à-bit |
| 6 | Gros fichier aléatoire | `/dev/urandom` 1 MB | `diff` bit-à-bit |

Tous les tests s'exécutent dans `/tmp`, nettoyage automatique via `trap cleanup EXIT`.

---

## `test_dedup.sh` — Déduplication

**Objectif** : Vérifier que deux backups successifs du même fichier ne créent pas de chunks en double.

| # | Test | Vérification |
|---|------|-------------|
| 1 | 1er backup stocke des chunks | `chunks_after_first > 0` |
| 2 | 2ème backup : pas de nouveaux chunks | `chunks_after_second == chunks_after_first` (WARN si non) |
| 3 | La taille du store ne croît pas | `store_size_second <= store_size_first` |
| 4 | Le restore après double backup est identique | `diff` bit-à-bit |

**Note** : `MemoryDedupIndex` est en mémoire, la déduplication ne traverse pas les exécutions. Les WARN sont attendus si l'index n'est pas implémenté.

---

## `test_edge_cases.sh` — Cas limites

**Objectif** : Tester les frontières du chunking (8 KB fixe) et les cas pathologiques.

| # | Test | Taille | Pourquoi |
|---|------|--------|----------|
| 1 | Fichier vide | 0 B | Répartition en zéro chunk |
| 2 | 1 octet | 1 B | Plus petit fichier non vide |
| 3 | Zéros (1 KB) | 1024 B | Tous les octets sont `\x00` |
| 4 | Motif "RMRF" répété | ~1 KB | Motif compressible, vérifie l'intégrité |
| 5 | Tous `\xFF` (1 KB) | 1024 B | Valeur maximale, test binaire pur |
| 6 | Nom de fichier Unicode | `écoß测试.txt` | Chemins non-ASCII |
| 7 | Taille de chunk exacte | 8192 B | Un seul chunk plein |
| 8 | Chunk + 1 octet | 8193 B | Franchit la limite de chunk |

---

## `test_sizes.sh` — Stress par tailles

**Objectif** : Valider que le pipeline tient la charge sur différentes tailles de fichiers, avec données aléatoires (incompressibles).

| Taille | Temps approximatif |
|--------|-------------------|
| 1 KB | Instantané |
| 8 KB | Instantané |
| 16 KB | Instantané |
| 64 KB | Instantané |
| 128 KB | Instantané |
| 256 KB | < 1s |
| 512 KB | < 1s |
| 1 MB | ~1s |
| 5 MB | ~5s |

Chaque test : `dd /dev/urandom` → backup → restore → `diff`. Les fichiers > 100 MB sont automatiquement sautés (`SKIP`).

---

## `test_errors.sh` — Gestion d'erreurs

**Objectif** : Vérifier que le programme gère proprement les erreurs (pas de crash, pas de corruption silencieuse).

| # | Scénario | Comportement attendu |
|---|----------|---------------------|
| 1 | Commande inconnue | Message d'erreur contenant "unknown" |
| 2 | Argument manquant (`backup` sans path) | Échec avec exit code ≠ 0 |
| 3 | Fichier source inexistant | Message d'erreur |
| 4 | Manifest inexistant | Message d'erreur |
| 5 | Manifest corrompu (fichier texte au lieu de bincode) | Message d'erreur |
| 6 | Store non accessible en écriture (`chmod 444`) | Message d'erreur |
| 7 | Chunk manquant (backup OK puis suppression des chunks) | Message d'erreur, pas de crash |
| 8 | Aucun argument | Message d'usage/erreur |

---

## `test_all.sh` — Runner global

Exécute séquentiellement :
1. `cargo build --workspace` (vérification build)
2. `cargo test --workspace` (tests unitaires Rust)
3. `test_basic.sh`
4. `test_dedup.sh`
5. `test_edge_cases.sh`
6. `test_sizes.sh`
7. `test_errors.sh`

Affiche un résumé coloré avec le nombre de scripts réussis/échoués.

**Utilisation** :
```bash
bash scripts/test_all.sh
```

**Nettoyage des artefacts générés par les scripts** :
```bash
cargo run -- clean
```
