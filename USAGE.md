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
      non  → zstd compress → LocalChunkStore → DedupIndex insert
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
└── index.bincode
```

- **manifests/** : un fichier `.bincode` par backup. Nom = `<UTC timestamp>-<basename>.bincode`
- **chunks/** : arborescence 2 niveaux. Les 2 premiers caractères du hash hex = dossier, reste = nom de fichier + `.zst` (compressé)
- **index.bincode** : index de déduplication persistant (HashMap sérialisée)

---

## 3. Commandes

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

#### VM (Proxmox/Libvirt)

```bash
rmrf backup /dev/pve/vm-100-disk-0 -s /path/to/store \
  --vm 100 \
  --hypervisor proxmox \
  --format qcow2 \
  --disk-name virtio0
```

La VM est suspendue avant la lecture du disque, puis relancée après. Le manifest contient les métadonnées VM (`vm_id`, `disk_name`, `format`).

#### Flags

| Flag | Description | Défaut |
|------|-------------|--------|
| `-s, --store <PATH>` | Répertoire du store | `./rmrf-store` |
| `--vm <ID>` | ID de la VM (active suspend/resume) | — |
| `--hypervisor <TYPE>` | `auto` (détecte), `proxmox`, `libvirt`, `noop` | `auto` |
| `--no-suspend` | Ne pas suspendre la VM (pour disque déjà snapshotté) | — |
| `--format <FMT>` | `auto` (détecte par extension), `raw`, `qcow2` | `auto` |
| `--disk-name <NAME>` | Nom du disque pour les métadonnées | basename du path |
| `--retain <DAYS>` | Supprime les manifests plus vieux que N jours, puis GC | — |

#### Rétention automatique

```bash
rmrf backup /path/to/file -s /path/to/store --retain 30
```

Après le backup, supprime les manifests âgés de plus de 30 jours, décrémente le refcount de leurs chunks, sauvegarde l'index, puis lance le GC.

---

### `restore` — Restaurer

```bash
rmrf restore <manifest> -s <store> [-o <output>] [--force]
```

#### Exemple

```bash
rmrf restore /path/to/store/manifests/20260706T174941Z-file.txt.bincode \
  -s /path/to/store \
  -o /tmp/restored.txt
```

Si `-o` est omis, le fichier est restauré dans le répertoire courant avec le nom d'origine (extrait du nom du manifest).

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

---

### `info` — Détails d'un backup

```bash
rmrf info <manifest> -s /path/to/store
```

Exemple sortie fichier :
```
Version:     v2
Source path: /path/to/file.txt
Source size: 25 (25 B)
Source type: File
Modified:    1783360186
Permissions: 100644
Chunks:      1
```

Exemple sortie VM :
```
Version:     v2
Source path: /mnt/pve/TrueNAS/images/103/vm-103-disk-1.qcow2
Source size: 3221946368 (3.0 GiB)
Source type: VmDisk { vm_id: "103", disk_name: "virtio0", format: Qcow2 }
Modified:    1783360186
Permissions: 100644
Chunks:      197
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

## 4. Workflow Proxmox complet

### Backup d'une VM en production

```bash
# 1. Identifier la VM
qm list
qm config 103                    # voir la config
qm status 103                    # running ou stopped

# 2. Trouver le chemin du disque
#    Dans "qm config 103", chercher la ligne scsi0/ide0/virtio0 :
#    scsi0: TrueNAS:103/vm-103-disk-1.qcow2,size=3G
pvesm path TrueNAS:103/vm-103-disk-1.qcow2
#    → /mnt/pve/TrueNAS/images/103/vm-103-disk-1.qcow2

# 3. Backup (la VM sera suspendue pendant la lecture)
rmrf backup /mnt/pve/TrueNAS/images/103/vm-103-disk-1.qcow2 \
  -s /var/lib/rmrf \
  --vm 103 \
  --hypervisor proxmox \
  --format qcow2 \
  --disk-name virtio0

# 4. Vérifier le backup
rmrf list -s /var/lib/rmrf
rmrf info <manifest> -s /var/lib/rmrf
rmrf verify <manifest> -s /var/lib/rmrf
```

### Restore d'une VM

```bash
# 1. Restaurer vers /tmp (JAMAIS vers le disque live)
rmrf restore <manifest> -s /var/lib/rmrf -o /tmp/vm-103-restored.qcow2 --force

# 2. Vérifier la taille
ls -lh /tmp/vm-103-restored.qcow2

# 3. Arrêter la VM
qm stop 103

# 4. Remplacer le disque (exemple avec TrueNAS NFS)
cp /tmp/vm-103-restored.qcow2 /mnt/pve/TrueNAS/images/103/vm-103-disk-1.qcow2

# 5. Redémarrer la VM
qm start 103
```

### Backup régulier avec rétention

```bash
# Cron : backup quotidien, conserve 30 jours
0 3 * * * rmrf backup /mnt/pve/TrueNAS/images/103/vm-103-disk-1.qcow2 \
  -s /var/lib/rmrf --vm 103 --hypervisor proxmox --format qcow2 --retain 30
```

### Mode sans suspension (snapshot externe)

Si tu as déjà un snapshot ZFS/LVM ou un disque inactif :

```bash
rmrf backup /mnt/snapshot/vm-103-disk-1.qcow2 \
  -s /var/lib/rmrf \
  --vm 103 \
  --no-suspend
```

---

## 5. FAQ

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

`auto` teste `qm status <vmid>` puis `virsh dominfo <vmid>`. Si ni `qm` ni `virsh` n'est installé, ou si la VM n'existe pas dans l'hyperviseur, il fallback vers `NoopLifecycle` (pas de suspend/resume). Force l'hyperviseur explicitement avec `--hypervisor proxmox` ou `--hypervisor libvirt`.
