#!/usr/bin/env node
/**
 * Inventory a skillset tree for router authoring.
 *
 * Usage:
 *   node list-members.mjs <skillset-name-or-dir> [--project <dir>] [--library <dir>]
 *   node list-members.mjs <...> --out <file.json>
 *   node list-members.mjs <...> --stdout full|summary   # default: summary
 *
 * Resolves project (.agents/skills/<name>) and library ($TINK_HOME/skills or ~/.tink/skills).
 * Full JSON defaults to ~/.tink/cache/manage-tink-skillset-router/<skillset>.<kind>.json
 * (never writes into the skillset tree — protects Tink digests).
 */
import {
	readdirSync,
	readFileSync,
	writeFileSync,
	existsSync,
	statSync,
	mkdirSync,
} from 'node:fs';
import { join, basename, resolve, dirname } from 'node:path';
import { homedir } from 'node:os';

function usage(code = 2) {
	console.error(`Usage: node list-members.mjs <skillset-name-or-dir> [options]
  --project <dir>     Project root (default: cwd). Looks in <dir>/.agents/skills/<name>
  --library <dir>     Library skills root (default: $TINK_HOME/skills or ~/.tink/skills)
  --out <file>        Write full JSON inventory (default: ~/.tink/cache/manage-tink-skillset-router/...)
  --stdout full|summary   What to print (default: summary)
  --all-trees         Inventory every resolved candidate tree`);
	process.exit(code);
}

function parseArgs(argv) {
	const out = {
		target: null,
		project: process.cwd(),
		library: null,
		out: null,
		stdout: 'summary',
		allTrees: false,
	};
	for (let i = 0; i < argv.length; i++) {
		const a = argv[i];
		if (a === '--project') out.project = argv[++i];
		else if (a === '--library') out.library = argv[++i];
		else if (a === '--out') out.out = argv[++i];
		else if (a === '--stdout') out.stdout = argv[++i];
		else if (a === '--all-trees') out.allTrees = true;
		else if (a === '-h' || a === '--help') usage(0);
		else if (a.startsWith('-')) {
			console.error(`Unknown flag: ${a}`);
			usage(2);
		} else if (!out.target) out.target = a;
		else {
			console.error(`Unexpected argument: ${a}`);
			usage(2);
		}
	}
	if (!out.target) usage(2);
	if (out.stdout !== 'full' && out.stdout !== 'summary') {
		console.error('--stdout must be full or summary');
		process.exit(2);
	}
	return out;
}

function defaultLibraryRoot() {
	const home = process.env.TINK_HOME?.trim();
	if (home) return join(home, 'skills');
	return join(homedir(), '.tink', 'skills');
}

function defaultCacheDir() {
	const home = process.env.TINK_HOME?.trim();
	const base = home || join(homedir(), '.tink');
	return join(base, 'cache', 'manage-tink-skillset-router');
}

function defaultInventoryPath(skillset, kind) {
	return join(defaultCacheDir(), `${skillset}.${kind || 'path'}.json`);
}

function resolveCandidates(target, projectRoot, libraryRoot) {
	const abs = resolve(target);
	if (existsSync(abs) && statSync(abs).isDirectory()) {
		return [{ kind: 'path', root: abs, skillset: basename(abs) }];
	}
	const name = target.replace(/\/$/, '');
	if (!name.endsWith('-skillset')) {
		console.error(`Refusing non-canonical skillset name (expected *-skillset): ${name}`);
		process.exit(2);
	}
	const candidates = [];
	const projectPath = join(resolve(projectRoot), '.agents', 'skills', name);
	const libraryPath = join(resolve(libraryRoot), name);
	if (existsSync(projectPath) && statSync(projectPath).isDirectory()) {
		candidates.push({ kind: 'project', root: projectPath, skillset: name });
	}
	if (existsSync(libraryPath) && statSync(libraryPath).isDirectory()) {
		candidates.push({ kind: 'library', root: libraryPath, skillset: name });
	}
	return candidates;
}

/** Minimal YAML frontmatter parser for skill name/description (no dependency). */
function parseFrontmatter(raw) {
	const text = raw.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n').replace(/\r/g, '\n');
	if (!text.startsWith('---\n') && text !== '---') {
		return { name: '', description: '', rawFrontmatter: '' };
	}
	const end = text.indexOf('\n---', 3);
	if (end === -1) return { name: '', description: '', rawFrontmatter: '' };
	const block = text.slice(4, end); // after ---\n
	const fields = {};
	let key = null;
	let style = 'plain'; // plain | folded | literal
	let buf = [];

	const flush = () => {
		if (!key) return;
		let v = buf.join('\n');
		if (style === 'folded') {
			// YAML folded >: collapse single newlines to spaces; keep blank-line paragraph breaks.
			v = v
				.replace(/\n[ \t]*\n/g, '\u0000')
				.replace(/\n+/g, ' ')
				.replace(/\u0000/g, '\n')
				.replace(/[ \t]+/g, ' ')
				.trim();
		} else if (style === 'literal') {
			v = v.replace(/\n$/, '');
		} else {
			v = v.trim();
			if (
				(v.startsWith('"') && v.endsWith('"')) ||
				(v.startsWith("'") && v.endsWith("'"))
			) {
				v = v.slice(1, -1);
			}
		}
		fields[key] = v.replace(/\s+/g, ' ').trim();
		key = null;
		style = 'plain';
		buf = [];
	};

	for (const line of block.split('\n')) {
		const keyMatch = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
		if (keyMatch && !/^[ \t]/.test(line)) {
			flush();
			key = keyMatch[1];
			const rest = keyMatch[2];
			if (rest === '>' || rest === '>-' || rest === '>+') {
				style = 'folded';
				buf = [];
			} else if (rest === '|' || rest === '|-' || rest === '|+') {
				style = 'literal';
				buf = [];
			} else if (rest === '') {
				style = 'plain';
				buf = [];
			} else {
				style = 'plain';
				buf = [rest];
			}
			continue;
		}
		if (key) {
			if (style === 'plain' && buf.length && !/^[ \t]/.test(line) && /:/.test(line)) {
				// Ambiguous; treat as continuation only when indented.
			}
			buf.push(line.replace(/^[ \t]+/, ''));
		}
	}
	flush();

	return {
		name: fields.name || '',
		description: fields.description || '',
		rawFrontmatter: block,
	};
}

function stripFrontmatter(raw) {
	const text = raw.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n').replace(/\r/g, '\n');
	if (!text.startsWith('---\n')) return text;
	const end = text.indexOf('\n---', 3);
	if (end === -1) return text;
	return text.slice(end + 4).replace(/^\n+/, '');
}

function extractOpener(body) {
	const lines = body.split('\n');
	let i = 0;
	while (i < lines.length && !lines[i].trim()) i++;
	const title = lines[i]?.startsWith('#') ? lines[i].replace(/^#+\s*/, '').trim() : '';
	if (lines[i]?.startsWith('#')) i++;
	while (i < lines.length && !lines[i].trim()) i++;
	const paras = [];
	let buf = [];
	for (; i < lines.length; i++) {
		const line = lines[i];
		if (line.startsWith('##')) break;
		if (!line.trim()) {
			if (buf.length) {
				paras.push(buf.join(' ').replace(/\s+/g, ' ').trim());
				buf = [];
				if (paras.length >= 2) break;
			}
			continue;
		}
		buf.push(line.trim());
	}
	if (buf.length && paras.length < 2) {
		paras.push(buf.join(' ').replace(/\s+/g, ' ').trim());
	}
	return { title, opener: paras.join('\n\n') };
}

function extractHandoffs(body, memberDirs) {
	const dirSet = new Set(memberDirs);
	const found = new Set();
	const reTick = /`([a-z0-9][a-z0-9-]{1,63})`/g;
	let m;
	while ((m = reTick.exec(body)) !== null) {
		if (dirSet.has(m[1])) found.add(m[1]);
	}
	const reLink = /\]\(([a-z0-9][a-z0-9-]{1,63})\/SKILL\.md\)/g;
	while ((m = reLink.exec(body)) !== null) {
		if (dirSet.has(m[1])) found.add(m[1]);
	}
	return [...found].sort();
}

function detectDeprecated(description, title, opener) {
	const hay = `${description}\n${title}\n${opener}`.toLowerCase();
	const deprecated =
		/\bdeprecated\b/.test(hay) ||
		/\bretired\b/.test(hay) ||
		/\bdo not invoke\b/.test(hay);
	let replacement = null;
	const rep =
		description.match(/\b(?:use|run|see)\s+`([a-z0-9][a-z0-9-]{1,63})`/i) ||
		description.match(/\binstead[^.]*\b`([a-z0-9][a-z0-9-]{1,63})`/i);
	if (rep) replacement = rep[1];
	return { deprecated, replacement };
}

/**
 * Role classification. Avoid false positives from worker copy that mentions
 * "orchestrator" as the caller to protect.
 */
function classifyRole({ name, description, title, opener, bodyHead, bodyFull }) {
	const desc = description.toLowerCase();
	const head = `${title}\n${opener}`.toLowerCase();
	const body = `${bodyHead}\n${bodyFull}`.toLowerCase();

	const { deprecated, replacement } = detectDeprecated(description, title, opener);
	if (deprecated) {
		return { role: 'deprecated', replacement, coordinatorSignals: [], notes: ['deprecated'] };
	}

	const workerShield =
		/orchestrator's context stays clean/.test(desc) ||
		/so the orchestrator's context/.test(desc) ||
		/main conversation's context stays clean/.test(desc) ||
		/runs? in a child agent so the main conversation/.test(desc) ||
		/report back to (?:your |the )?orchestrator/.test(desc) ||
		/report back to (?:your |the )?orchestrator/.test(body) ||
		(/delegate (?:noisy|heavy|investigation)/.test(desc) &&
			/orchestrator/.test(desc) &&
			!/\byou act as the orchestrator\b/.test(head));

	const coordinatorSignals = [];
	const tests = [
		[/combines all of the/, 'combines all'],
		[/cross-discipline/, 'cross-discipline'],
		[/holistic (?:interface )?review/, 'holistic review'],
		[/single review across/, 'single review across'],
		[/\byou act as the (?:orchestrator|coordinator)\b/, 'self-orchestrator'],
		[/using an orchestrator agent and a fleet/, 'saga-orchestrator'],
		[/fleet of worker/, 'worker-fleet'],
		[/subagent council|model-diverse.*council|council of/, 'council'],
		[/coordinate[sd]? (?:multiple )?subagents/, 'coordinates-subagents'],
		[/drives? a .*workflow|end-to-end workflow|spec-driven development "saga"/, 'workflow-driver'],
		[/routes the (?:interface|request) to each/, 'routes-to-each'],
	];

	for (const [re, label] of tests) {
		if (re.test(desc) || re.test(head)) coordinatorSignals.push(label);
	}

	// Body-only "Orchestrator workflow" headings often mean "how the caller uses me".
	const bodyClaimsSelf =
		/\byou act as the (?:orchestrator|coordinator)\b/.test(body) ||
		/\bas the orchestrator,?\s+you\b/.test(body);

	if (workerShield && !coordinatorSignals.length && !bodyClaimsSelf) {
		return {
			role: 'worker',
			replacement: null,
			coordinatorSignals: [],
			notes: ['mentions-orchestrator-as-caller'],
		};
	}

	if (coordinatorSignals.length || bodyClaimsSelf) {
		return {
			role: 'coordinator',
			replacement: null,
			coordinatorSignals: bodyClaimsSelf
				? [...new Set([...coordinatorSignals, 'self-orchestrator'])]
				: coordinatorSignals,
			notes: workerShield ? ['also-mentions-caller-orchestrator'] : [],
		};
	}

	return { role: 'leaf', replacement: null, coordinatorSignals: [], notes: [] };
}

function suggestCluster(description, name) {
	const hay = `${name} ${description}`.toLowerCase();
	const rules = [
		['Specs & planning', /\bspec\b|product\.md|tech\.md|\bsaga\b|implement-specs|spec-driven|write-product|write-tech/],
		['PR lifecycle', /\bpr\b|pull request|review-pr|create-pr|walkthrough|merge conflict|rebase/],
		['Diagnostics & fixing', /diagnos|ci failure|fix-error|fix errors|bug report|reproduce|clipp|compil|merge conflict/],
		['Docs & readout', /docs|documentation|readout|mdx|feature-docs/],
		['Research & critique', /research|council|cross-critique|investigat/],
		['Meta & feedback', /skill-doctor|update-skill|complain|suggestion-box|brandalf/],
	];
	for (const [label, re] of rules) {
		if (re.test(hay)) return label;
	}
	return 'General';
}

function inventoryTree(root) {
	const skillset = basename(root);
	const receiptPath = join(root, '.tink-skillset.json');
	let receiptMembers = null;
	if (existsSync(receiptPath)) {
		try {
			const receipt = JSON.parse(readFileSync(receiptPath, 'utf8'));
			if (Array.isArray(receipt.members)) receiptMembers = [...receipt.members].sort();
		} catch {
			receiptMembers = null;
		}
	}

	const entries = readdirSync(root)
		.filter((name) => {
			if (name.startsWith('.')) return false;
			if (name === 'SKILL.md') return false;
			const p = join(root, name);
			return statSync(p).isDirectory() && existsSync(join(p, 'SKILL.md'));
		})
		.sort();

	const members = entries.map((dir) => {
		const skillPath = join(root, dir, 'SKILL.md');
		const raw = readFileSync(skillPath, 'utf8');
		const { name, description } = parseFrontmatter(raw);
		const body = stripFrontmatter(raw);
		const { title, opener } = extractOpener(body);
		const bodyHead = body.slice(0, 6000);
		const roleInfo = classifyRole({
			name: name || dir,
			description: description || '',
			title,
			opener,
			bodyHead,
			bodyFull: body,
		});
		const handoffs = extractHandoffs(body, entries).filter((h) => h !== dir);
		return {
			dir,
			name: name || dir,
			description: description || '',
			path: `${dir}/SKILL.md`,
			title,
			opener,
			handoffs,
			role: roleInfo.role,
			replacement: roleInfo.replacement,
			coordinatorSignals: roleInfo.coordinatorSignals,
			notes: roleInfo.notes,
			clusterHint: suggestCluster(description || '', dir),
		};
	});

	const onDisk = new Set(entries);
	const inReceipt = new Set(receiptMembers || []);
	const missingFromDisk =
		receiptMembers == null ? [] : receiptMembers.filter((m) => !onDisk.has(m));
	const orphanedOnDisk =
		receiptMembers == null ? [] : entries.filter((m) => !inReceipt.has(m));

	const byRole = {
		coordinator: members.filter((m) => m.role === 'coordinator').map((m) => m.dir),
		worker: members.filter((m) => m.role === 'worker').map((m) => m.dir),
		deprecated: members.filter((m) => m.role === 'deprecated').map((m) => m.dir),
		leaf: members.filter((m) => m.role === 'leaf').map((m) => m.dir),
	};

	const clusters = {};
	for (const m of members) {
		if (m.role === 'deprecated') continue;
		(clusters[m.clusterHint] ||= []).push(m.dir);
	}

	return {
		skillset,
		root,
		hasReceipt: existsSync(receiptPath),
		hasRootSkill: existsSync(join(root, 'SKILL.md')),
		receiptMembers,
		receiptDiff: {
			missingFromDisk,
			orphanedOnDisk,
		},
		counts: {
			members: members.length,
			coordinators: byRole.coordinator.length,
			workers: byRole.worker.length,
			deprecated: byRole.deprecated.length,
			leaf: byRole.leaf.length,
		},
		byRole,
		clusters,
		members,
	};
}

function summarize(inv) {
	return {
		skillset: inv.skillset,
		root: inv.root,
		hasReceipt: inv.hasReceipt,
		hasRootSkill: inv.hasRootSkill,
		counts: inv.counts,
		byRole: inv.byRole,
		receiptDiff: inv.receiptDiff,
		clusters: inv.clusters,
		members: inv.members.map((m) => ({
			dir: m.dir,
			role: m.role,
			clusterHint: m.clusterHint,
			replacement: m.replacement,
			coordinatorSignals: m.coordinatorSignals,
			description: m.description.slice(0, 140) + (m.description.length > 140 ? '…' : ''),
		})),
		inventoryFile: inv.inventoryFile,
	};
}

const args = parseArgs(process.argv.slice(2));
const libraryRoot = args.library || defaultLibraryRoot();
const candidates = resolveCandidates(args.target, args.project, libraryRoot);

if (!candidates.length) {
	console.error(`No skillset found for ${args.target}`);
	console.error(`  project: ${join(resolve(args.project), '.agents', 'skills', basename(args.target))}`);
	console.error(`  library: ${join(resolve(libraryRoot), basename(args.target))}`);
	process.exit(1);
}

const chosen =
	args.allTrees || candidates.length === 1
		? candidates
		: // Prefer project when both exist and user did not ask for all.
			candidates.filter((c) => c.kind === 'project').length
			? candidates.filter((c) => c.kind === 'project')
			: candidates;

const inventories = chosen.map((c) => {
	const inv = inventoryTree(c.root);
	inv.kind = c.kind;
	const outPath = args.out
		? resolve(args.out)
		: defaultInventoryPath(inv.skillset, c.kind);
	let dest = outPath;
	if (args.out && chosen.length > 1) {
		dest = join(dirname(outPath), `${inv.skillset}.${c.kind}.json`);
	} else if (!args.out && chosen.length > 1) {
		dest = defaultInventoryPath(inv.skillset, c.kind);
	}
	inv.inventoryFile = dest;
	mkdirSync(dirname(dest), { recursive: true });
	writeFileSync(dest, `${JSON.stringify(inv, null, 2)}\n`);
	return inv;
});

const payload =
	inventories.length === 1
		? args.stdout === 'full'
			? inventories[0]
			: summarize(inventories[0])
		: {
				candidates: candidates.map((c) => ({ kind: c.kind, root: c.root })),
				inventories: inventories.map((inv) =>
					args.stdout === 'full' ? inv : summarize(inv),
				),
			};

process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
