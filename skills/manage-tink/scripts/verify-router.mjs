#!/usr/bin/env node
/**
 * Verify a skillset-root router SKILL.md against an inventory.
 *
 * Usage:
 *   node verify-router.mjs <skillset-dir> [--inventory <file>]
 *
 * Exit 0 = pass, 1 = fail, 2 = usage/input error.
 */
import { readFileSync, existsSync, writeFileSync } from 'node:fs';
import { join, resolve, basename } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));

function usage(code = 2) {
	console.error('Usage: node verify-router.mjs <skillset-dir> [--inventory <file>]');
	process.exit(code);
}

function parseArgs(argv) {
	let root = null;
	let inventory = null;
	for (let i = 0; i < argv.length; i++) {
		if (argv[i] === '--inventory') inventory = argv[++i];
		else if (argv[i].startsWith('-')) usage(2);
		else if (!root) root = argv[i];
		else usage(2);
	}
	if (!root) usage(2);
	return { root: resolve(root), inventory };
}

function parseFrontmatter(raw) {
	const text = raw.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n').replace(/\r/g, '\n');
	if (!text.startsWith('---\n')) return { name: '', description: '', body: text };
	const end = text.indexOf('\n---', 3);
	if (end === -1) return { name: '', description: '', body: text };
	const block = text.slice(4, end);
	const body = text.slice(end + 4).replace(/^\n+/, '');
	const fields = {};
	let key = null;
	let buf = [];
	let folded = false;
	const flush = () => {
		if (!key) return;
		let v = buf.join('\n');
		if (folded) {
			v = v
				.replace(/\n[ \t]*\n/g, '\u0000')
				.replace(/\n+/g, ' ')
				.replace(/\u0000/g, '\n')
				.replace(/[ \t]+/g, ' ')
				.trim();
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
		buf = [];
		folded = false;
	};
	for (const line of block.split('\n')) {
		const m = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
		if (m && !/^[ \t]/.test(line)) {
			flush();
			key = m[1];
			const rest = m[2];
			if (rest === '>' || rest === '>-' || rest === '>+') {
				folded = true;
				buf = [];
			} else if (rest === '' || rest === '|' || rest === '|-' || rest === '|+') {
				folded = rest.startsWith('|');
				buf = [];
			} else buf = [rest];
			continue;
		}
		if (key) buf.push(line.replace(/^[ \t]+/, ''));
	}
	flush();
	return {
		name: fields.name || '',
		description: fields.description || '',
		body,
	};
}

function loadInventory(root, inventoryPath) {
	if (inventoryPath) {
		return JSON.parse(readFileSync(resolve(inventoryPath), 'utf8'));
	}
	const listScript = join(__dirname, 'list-members.mjs');
	const r = spawnSync(process.execPath, [listScript, root, '--stdout', 'summary'], {
		encoding: 'utf8',
	});
	if (r.status !== 0) {
		console.error(r.stderr || r.stdout);
		process.exit(2);
	}
	let summary;
	try {
		summary = JSON.parse(r.stdout);
	} catch {
		console.error('list-members did not return JSON summary');
		process.exit(2);
	}
	const invFile = summary.inventoryFile;
	if (!invFile || !existsSync(invFile)) {
		console.error('inventory file missing after list-members');
		process.exit(2);
	}
	return JSON.parse(readFileSync(invFile, 'utf8'));
}

function extractLinkedMembers(body) {
	const links = new Set();
	const re = /\]\(([a-z0-9][a-z0-9-]*)\/SKILL\.md\)/g;
	let m;
	while ((m = re.exec(body)) !== null) links.add(m[1]);
	return links;
}

function sectionBody(body, heading) {
	// Match "## Classify..." or "## 1. Classify..."
	const re = new RegExp(
		`##\\s*(?:\\d+[.:)]\\s*)?${heading}\\n([\\s\\S]*?)(?=\\n##\\s|$)`,
		'i',
	);
	const m = body.match(re);
	return m ? m[1] : '';
}

const { root, inventory: invArg } = parseArgs(process.argv.slice(2));
const routerPath = join(root, 'SKILL.md');
const failures = [];
const warnings = [];

if (!existsSync(routerPath)) {
	failures.push('missing root SKILL.md');
	console.log(JSON.stringify({ ok: false, failures, warnings }, null, 2));
	process.exit(1);
}

const inv = loadInventory(root, invArg);
const raw = readFileSync(routerPath, 'utf8');
const { name, description, body } = parseFrontmatter(raw);
const expectedName = basename(root);
const lineCount = raw.replace(/\r\n/g, '\n').split('\n').length;

if (name !== expectedName) {
	failures.push(`frontmatter name "${name}" !== directory basename "${expectedName}"`);
}
if (!description.trim()) failures.push('empty description');
if (!/do not use when/i.test(description) && !/already the clear owner/i.test(description)) {
	failures.push('description missing negative trigger for already-named members');
}

const linked = extractLinkedMembers(body);
const active = inv.members.filter((m) => m.role !== 'deprecated');
const deprecated = inv.members.filter((m) => m.role === 'deprecated');
const activeDirs = new Set(active.map((m) => m.dir));
const deprecatedDirs = new Set(deprecated.map((m) => m.dir));

for (const m of active) {
	if (!linked.has(m.dir)) {
		failures.push(`active member not linked in router: ${m.dir}`);
	}
}

const boundaries = sectionBody(body, 'Boundaries');
for (const m of deprecated) {
	const mentioned =
		linked.has(m.dir) ||
		boundaries.includes(m.dir) ||
		boundaries.includes(`\`${m.dir}\``) ||
		body.includes(m.dir);
	if (!mentioned) {
		failures.push(`deprecated member ${m.dir} not mentioned in Boundaries (or elsewhere)`);
	}
	// Prefer not in classify table — soft fail if only in boundaries
	const classify = sectionBody(body, 'Classify the request');
	if (new RegExp(`\\]\\(${m.dir}/SKILL\\.md\\)`).test(classify)) {
		warnings.push(
			`deprecated member ${m.dir} still appears in Classify table; prefer Boundaries only`,
		);
	}
}

for (const dir of linked) {
	if (!activeDirs.has(dir) && !deprecatedDirs.has(dir)) {
		failures.push(`router links unknown member: ${dir}`);
	}
}

// Duplicate links among active rows
const linkCounts = {};
for (const dir of linked) {
	linkCounts[dir] = (linkCounts[dir] || 0) + 1;
}
// Count occurrences in body
for (const m of active) {
	const re = new RegExp(`\\]\\(${m.dir}/SKILL\\.md\\)`, 'g');
	const n = (body.match(re) || []).length;
	if (n > 1) warnings.push(`member ${m.dir} linked ${n} times`);
}

const budget =
	active.length <= 10 ? 90 : active.length <= 20 ? 120 : 40 + active.length * 3 + 40;
if (lineCount > budget) {
	warnings.push(
		`router is ${lineCount} lines; soft budget for ${active.length} active members is ~${budget}. Prefer clustered sections.`,
	);
}

const hasClusters = /### /.test(sectionBody(body, 'Classify the request'));
if (active.length >= 12 && !hasClusters) {
	warnings.push('12+ active members without ### cluster headings under Classify');
}

const coords = inv.byRole?.coordinator || [];
if (coords.length > 1) {
	const classify = sectionBody(body, 'Classify the request');
	const scope = classify || body;
	const missingCoord = coords.filter((c) => !scope.includes(`${c}/SKILL.md`));
	if (missingCoord.length) {
		failures.push(`coordinator(s) missing from Classify: ${missingCoord.join(', ')}`);
	}
	if (!/###\s*Workflow coordinators/i.test(body) && !/workflow coordinators/i.test(body)) {
		warnings.push(
			'multiple coordinators detected; prefer a ### Workflow coordinators subsection',
		);
	}
}

// Heuristic: long pasted procedure blocks
if ((body.match(/^\d+\. /gm) || []).length > 20) {
	warnings.push('many numbered steps in router body; may be copying member procedures');
}

const result = {
	ok: failures.length === 0,
	router: routerPath,
	skillset: expectedName,
	lineCount,
	budget,
	linkedCount: linked.size,
	activeCount: active.length,
	deprecatedCount: deprecated.length,
	failures,
	warnings,
};

process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
process.exit(failures.length ? 1 : 0);
