# RMRF — Guide d'utilisation

## 1. Installation

### Build depuis les sources

```bash
git clone <repo> RMRF
cd RMRF
cargo build --release
```

Le binaire se trouve dans `target/release/rmrf`.

### Installation système

```bash
sudo cp target/release/rmrf /usr/local/bin/
rmrf --help
```

> **Note Proxmox** : si le binaire compilé sur une autre machine refuse de démarrer (erreur `GLIBC_2.39 not found`), compilez directement sur le Proxmox :
> ```bash
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
> source ~/.cargo/env
> cd ~/RMRF && cargo build --release
> cp target/release/rmrf /usr/local/bin/
> ```

---

## 2. Concepts

| Terme | Définition |
|-------|-----------|
| **Store** | Répertoire contenant les chunks, manifests et index de déduplication |
| **Chunk** | Bloc de données variable (4 KB à 64 KB) produit par FastCDC |
| **Manifest** | Fichier `.bincode` listant les chunks composant un backup |
| **Déduplication** | Deux chunks identiques (même hash BLAKE3) ne sont stockés qu'une fois |
| **Index** | `index.bincode` — mapping hash → chunk, persistant entre les exécutions |

### Pipeline de backup

```
Source (fichier/disk) → FastCDC (chunks variables) → BLAKE3 (hash)
  → DedupIndex (déjà stocké ?)
      oui → incrémenter refcount, skip
      non  → [age encrypt →] zstd compress → LocalChunkStore → DedupIndex insert
  → Manifest (liste des hashes) → manifests/<timestamp>-<nom>.bincode
```

### Structure du store

```
<store>/
├── manifests/
│   ├── 20260706T174941Z-file.txt.bincode
│   └── 20260706T174941Z-a.txt.bincode
├── chunks/
│   └── ab/
│       └── cdef0123456789...ff.zst
├── index.bincode
└── .rmrf.lock
```

- **manifests/** : un fichier `.bincode` par backup. Nom = `<UTC timestamp>-<basename>.bincode`
- **chunks/** : arborescence 2 niveaux. Les 2 premiers caractères du hash hex = dossier, reste = nom de fichier + `.zst` (compressé)
- **index.bincode** : index de déduplication persistant (HashMap sérialisée)
- **.rmrf.lock** : verrou PID-based pour éviter les accès concurrents

---

## 3. Configuration

RMRF lit `~/.rmrf.toml` s'il existe. Tous les champs sont optionnels :

```toml
store = "/var/lib/rmrf"
compression_level = 10
cdc_min = 4096
cdc_avg = 16384
cdc_max = 65536
```

| Champ | Défaut | Description |
|-------|--------|-------------|
| `store` | `./rmrf-store` | Chemin du store par défaut |
| `compression_level` | `10` | Niveau zstd (1-22) |
| `cdc_min` | `4096` | Taille minimale de chunk (bytes) |
| `cdc_avg` | `16384` | Taille moyenne cible de chunk (bytes) |
| `cdc_max` | `65536` | Taille maximale de chunk (bytes) |

Surcharge possible avec `--config <path>` :

```bash
rmrf --config /etc/rmrf.toml backup /path/to/file
```

---

## 4. Commandes

### `backup` — Sauvegarder

#### Fichier simple

```bash
rmrf backup /path/to/file.txt -s /path/to/store
```

Le fichier est découpé en chunks, compressé, dédupliqué, et un manifest est créé.

#### Dossier (récursif)

```bash
rmrf backup /path/to/directory -s /path/to/store
```

Tous les fichiers du dossier (et sous-dossiers) sont sauvegardés. **Un manifest par fichier.** Les fichiers cachés (commençant par `.`) sont ignorés.

#### VM Proxmox (auto-détection du disque)

```bash
rmrf backup --vm 103 -s /path/to/store
```

Le chemin du disque est détecté automatiquement via `qm config` + `pvesm path`. La VM est suspendue avant la lecture, puis relancée.

Pour sélectionner un disque spécifique :

```bash
rmrf backup --vm 103 --disk scsi0 -s /path/to/store
```

#### VM Proxmox (chemin explicite)

```bash
rmrf backup /dev/pve/vm-100-disk-0 -s /path/to/store \
  --vm 100 --hypervisor proxmox --format qcow2
```

#### LXC (conteneur Proxmox)

```bash
rmrf backup --vm 100 --hypervisor proxmox-lxc -s /path/to/store
```

#### Live backup (QEMU QMP, sans suspend)

```bash
rmrf backup --vm 103 --live --qmp-socket /var/run/qemu-server/103.qmp \
  -s /path/to/store
```

Crée un snapshot QEMU via QMP (`blockdev-snapshot-sync`), backup le snapshot, puis le nettoie. La VM reste running.

#### Backup chiffré (age)

```bash
# Générer une clé age
age-keygen -o ~/.rmrf-key.txt

# Backup avec chiffrement
rmrf backup /path/to/file -s /path/to/store \
  --encrypt "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p"
```

Le manifest est marqué `encryption: age` et les chunks sont chiffrés avant stockage.

#### Flags

| Flag | Description | Défaut |
|------|-------------|--------|
| `-s, --store <PATH>` | Répertoire du store | `./rmrf-store` (ou config) |
| `--vm <ID>` | ID de la VM (active suspend/resume) | — |
| `--disk <SPEC>` | Nom du disque (scsi0, virtio0, rootfs, mp0...) | auto-détection |
| `--hypervisor <TYPE>` | `auto`, `proxmox`, `proxmox-lxc`, `libvirt`, `noop` | `auto` |
| `--no-suspend` | Ne pas suspendre la VM | — |
| `--live` | Backup à chaud via QMP snapshot | — |
| `--qmp-socket <PATH>` | Chemin du socket QMP (ex: `/var/run/qemu-server/103.qmp`) | — |
| `--format <FMT>` | `auto`, `raw`, `qcow2` | `auto` |
| `--encrypt <PUBKEY>` | Clé publique age pour chiffrer les chunks | — |
| `--retain <DAYS>` | Supprime les manifests plus vieux que N jours, puis GC | — |

#### Rétention automatique

```bash
rmrf backup /path/to/file -s /path/to/store --retain 30
```

Après le backup, supprime les manifests âgés de plus de 30 jours, décrémente le refcount de leurs chunks, sauvegarde l'index, puis lance le GC.

---

### `restore` — Restaurer

```bash
rmrf restore <manifest> -s <store> [-o <output>] [--force] [--jobs <N>]
```

#### Exemple

```bash
rmrf restore /path/to/store/manifests/20260706T174941Z-file.txt.bincode \
  -s /path/to/store \
  -o /tmp/restored.txt
```

Si `-o` est omis, le fichier est restauré dans le répertoire courant avec le nom d'origine (extrait du nom du manifest).

#### Restauration parallèle

```bash
rmrf restore <manifest> -s <store> -o /tmp/restored --jobs 8
```

Les chunks sont téléchargés en parallèle par lots de 64 (bounded RAM). Défaut : 4 jobs.

#### Restauration chiffrée

```bash
rmrf restore <manifest> -s <store> -o /tmp/restored \
  --identity ~/.rmrf-key.txt
```

Le manifest chiffré est détecté automatiquement. La clé privée age est nécessaire.

#### Safety : `--force`

Si le fichier de sortie existe déjà, `restore` **refuse** d'écrire sans `--force` :

```bash
rmrf restore <manifest> -s <store> -o /tmp/existing.txt
# Error: output file already exists: /tmp/existing.txt (use --force to overwrite)

rmrf restore <manifest> -s <store> -o /tmp/existing.txt --force
# OK: overwrite
```

> **⚠️ NE JAMAIS restaurer vers le disque live d'une VM en production.** Restaurer toujours vers `/tmp/` puis remplacer le disque avec `qm disk replace` après avoir arrêté la VM.

---

### `list` — Lister les backups

```bash
rmrf list -s /path/to/store
```

Affiche tous les manifests du store :

```
20260706T174941Z        12 B     1 chunks  /path/to/file.txt
20260706T174941Z         7 B     1 chunks  /path/to/a.txt
20260706T174941Z         7 B     1 chunks  /path/to/sub/b.txt
```

Colonnes : timestamp UTC | taille source | nombre de chunks | chemin source

#### Filtres

```bash
rmrf list -s /path/to/store --since 20260701 --before 20260731
rmrf list -s /path/to/store --source "vm-103"
```

| Flag | Description |
|------|-------------|
| `--since <YYYYMMDD>` | Uniquement les backups après cette date |
| `--before <YYYYMMDD>` | Uniquement les backups avant cette date |
| `--source <TEXT>` | Filtre par sous-chaîne dans le chemin source |

---

### `info` — Détails d'un backup

```bash
rmrf info <manifest> -s /path/to/store
```

Exemple sortie fichier :
```
Version:     v3
Source path: /path/to/file.txt
Source size: 25 (25 B)
Source type: File
Modified:    1783360186
Permissions: 100644
Chunks:      1
```

Exemple sortie VM :
```
Version:     v3
Source path: /mnt/pve/TrueNAS/images/103/vm-103-disk-1.qcow2
Source size: 3221946368 (3.0 GiB)
Source type: VmDisk { vm_id: "103", disk_name: "virtio0", format: Qcow2 }
Modified:    1783360186
Permissions: 100644
Chunks:      197
```

Exemple sortie backup chiffré :
```
Version:     v3
Encryption:  age
Source path: /path/to/secret.txt
...
```

---

### `verify` — Vérifier l'intégrité

```bash
rmrf verify <manifest> -s /path/to/store
```

Vérifie que tous les chunks référencés par le manifest existent sur le disque.

```
OK: all 197 chunks present and verified
```

Si un chunk manque :
```
MISSING: chunk 36/84db6b22... not found in store
Error: 1 chunk(s) missing
```

#### Vérification forte (re-hachage BLAKE3)

```bash
rmrf verify <manifest> -s /path/to/store --rehash
```

Décompresse et re-hache chaque chunk avec BLAKE3 pour vérifier l'intégrité des données (pas seulement la présence). Détecte la corruption silencieuse.

```
OK: all 197 chunks present and verified (re-hashed)
```

---

### `gc` — Nettoyer les chunks orphelins

```bash
rmrf gc -s /path/to/store --dry-run    # Voir ce qui serait supprimé
rmrf gc -s /path/to/store              # Supprimer réellement
```

Le GC scanne tous les manifests, collecte les hashes référencés, puis supprime les chunks sur le disque qui ne sont référencés par aucun manifest.

Sortie `--dry-run` :
```
Dry run: would delete 0 orphan chunks, would free 0 B
```

Sortie réelle :
```
Deleted orphan: 36/84db6b22cae9e719794c20f84d3cffa79d4c46e1b507d86e87491f85d4dfb5 (15 B)
Deleted 1 orphan chunks, freed 15 B
```

---

### `clean` — Nettoyer les artefacts de test

```bash
rmrf clean
```

Supprime les fichiers de test créés dans le répertoire courant par les scripts de test (`empty`, `one_byte`, `zeros`, `restored`, etc.). Utile après `bash scripts/test_all.sh`.

---

## 5. Workflow Proxmox complet

### Backup d'une VM en production (auto-détection)

```bash
# 1. Backup (auto-détecte le disque, suspend la VM)
rmrf backup --vm 103 -s /var/lib/rmrf

# 2. Vérifier le backup
rmrf list -s /var/lib/rmrf
rmrf info <manifest> -s /var/lib/rmrf
rmrf verify <manifest> -s /var/lib/rmrf
```

### Backup d'un conteneur LXC

```bash
rmrf backup --vm 100 --hypervisor proxmox-lxc -s /var/lib/rmrf
```

### Backup live (QMP, VM reste running)

```bash
rmrf backup --vm 103 --live --qmp-socket /var/run/qemu-server/103.qmp \
  -s /var/lib/rmrf
```

### Backup chiffré

```bash
# Générer la clé
age-keygen -o ~/.rmrf-key.txt
PUBKEY=$(grep "^# public key:" ~/.rmrf-key.txt | cut -d' ' -f4)

# Backup chiffré
rmrf backup --vm 103 -s /var/lib/rmrf --encrypt "$PUBKEY"

# Restauration
rmrf restore <manifest> -s /var/lib/rmrf -o /tmp/restored.qcow2 \
  --identity ~/.rmrf-key.txt
```

### Restore d'une VM

```bash
# 1. Restaurer vers /tmp (JAMAIS vers le disque live)
rmrf restore <manifest> -s /var/lib/rmrf -o /tmp/vm-103-restored.qcow2 --force

# 2. Vérifier la taille
ls -lh /tmp/vm-103-restored.qcow2

# 3. Arrêter la VM
qm stop 103

# 4. Remplacer le disque
cp /tmp/vm-103-restored.qcow2 /mnt/pve/TrueNAS/images/103/vm-103-disk-1.qcow2

# 5. Redémarrer la VM
qm start 103
```

### Backup régulier avec rétention

```bash
# Cron : backup quotidien, conserve 30 jours
0 3 * * * rmrf backup --vm 103 -s /var/lib/rmrf --retain 30
```

### Mode sans suspension (snapshot externe)

```bash
rmrf backup /mnt/snapshot/vm-103-disk-1.qcow2 \
  -s /var/lib/rmrf \
  --vm 103 \
  --no-suspend
```

---

## 6. FAQ

### "Error reading manifests directory: No such file or directory"

Le store n'existe pas ou n'est pas le bon. Spécifie le store avec `-s` :

```bash
rmrf list -s /var/lib/rmrf
```

### "GLIBC_2.39 not found"

Le binaire a été compilé sur une machine avec une glibc plus récente que la cible. Compile directement sur la machine cible (voir section Installation).

### La VM est restée suspendue après un crash

Si `rmrf` crash après `qm suspend` mais avant `qm resume`, la VM reste suspendue. La relancer manuellement :

```bash
qm resume <vmid>
```

> Le script `test_proxmox.sh` inclut une safety resume dans son cleanup.

### Combien d'espace disque le store utilise-t-il ?

Le store contient les chunks **compressés** et **dédupliqués**. Pour une VM de 3.2 GB qcow2, le store fait ~340 MB (10.5% de l'original). Les backups successifs de la même VM ne stockent que les chunks modifiés.

### Puis-je restaurer un seul fichier depuis un backup VM ?

Non. Le backup VM est au niveau disque (block-level), pas fichier (file-level). Pour restaurer un fichier : monter le qcow2 restauré et copier le fichier.

```bash
rmrf restore <manifest> -s /var/lib/rmrf -o /tmp/restored.qcow2 --force
modprobe nbd
qemu-nbd --connect=/dev/nbd0 /tmp/restored.qcow2
mount /dev/nbd0p1 /mnt
cp /mnt/path/to/file /tmp/
umount /mnt
qemu-nbd --disconnect /dev/nbd0
```

### `--hypervisor auto` ne détecte rien

`auto` teste `qm status <vmid>` puis `pct status <vmid>` puis `virsh dominfo <vmid>`. Si aucun hyperviseur n'est trouvé, il fallback vers `NoopLifecycle` (pas de suspend/resume). Force l'hyperviseur explicitement avec `--hypervisor proxmox`, `--hypervisor proxmox-lxc` ou `--hypervisor libvirt`.

### Comment générer une clé age ?

```bash
# Installer age (une fois)
apt install age       # Debian/Ubuntu
pacman -S age         # Arch
cargo install age     # depuis les sources

# Générer une clé
age-keygen -o ~/.rmrf-key.txt

# La clé publique est dans le commentaire
grep "^# public key:" ~/.rmrf-key.txt
```

### Le backup live QMP ne fonctionne pas

Vérifie que :
1. Le socket QMP existe : `ls -l /var/run/qemu-server/<vmid>.qmp`
2. Le device QEMU est correct : `--disk scsi0` (ou `virtio0`, `ide0`...)
3. QEMU est configuré avec `-qmp unix:/var/run/qemu-server/<vmid>.qmp,server,nowait`
4. Le format du disque est supporté (qcow2 recommandé pour les snapshots)
