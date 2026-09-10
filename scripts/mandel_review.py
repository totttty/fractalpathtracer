"""Explicit visual decisions, bound to immutable source and capture evidence."""
import hashlib
import json

DECISIONS = ('accepted', 'accepted-with-limitations', 'needs-work')


def evidence_digest(row):
    return hashlib.sha256(json.dumps(row, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def validate_reviews(reviews, evidence, sources):
    if reviews.get('version') != 1 or evidence.get('version') != 1:
        raise ValueError('unsupported visual-review schema')
    by_id = {}
    for row in evidence['rows']:
        key = int(row['id'])
        source = sources.get(key)
        if (key in by_id or source is None or row['path'] != source['path']
                or row['sha256'] != source['sha256'] or row['id'] != source['id']):
            raise ValueError('evidence identity mismatch')
        if set(row['captures']) != {'mandel','geometry','authored'} or row['samples'] != 32:
            raise ValueError('review requires all three captures and 32 FPT SPP')
        by_id[key] = row
    decisions = {}
    for review in reviews['rows']:
        key = int(review['id'])
        row = by_id.get(key)
        if (key in decisions or row is None or review['id'] != row['id']
                or review['source_sha256'] != row['sha256']
                or review['evidence_sha256'] != evidence_digest(row)):
            raise ValueError('stale, duplicate or mismatched visual decision')
        if (review['decision'] not in DECISIONS or not review['note'].strip()
                or not review['reviewer'].strip() or not review['reviewed_at'].strip()):
            raise ValueError('incomplete visual decision')
        if review['decision'] != 'needs-work' and (review['geometry'] != 'acceptable'
                or review['illumination'] != 'acceptable'):
            raise ValueError('promotion requires acceptable geometry and illumination')
        decisions[key] = review
    return decisions, by_id


def apply_reviews(catalog, reviews, evidence):
    from mandel_catalog import source_index
    decisions, by_id = validate_reviews(reviews,evidence,source_index(catalog['scenes']))
    for row in catalog['scenes']:
        review = decisions.get(int(row['id']))
        if review is None:
            # Publication alone cannot grant a new visual acceptance status.
            row['status'] = 'blocked' if row['blockers'] else 'experimental'
            continue
        row['visual_decision'] = review
        row['review_evidence'] = by_id[int(row['id'])]
        row['historical_blockers'] = row['blockers']
        row['status'] = 'blocked' if review['decision'] == 'needs-work' else 'reviewed'
        row['blockers'] = [review['note']] if review['decision'] == 'needs-work' else []
        row['review'] = 'explicit-visual-review'
        row['review_note'] = review['note']
        row['review_reference_overrides'] = by_id[int(row['id'])]['reference_overrides']
        row['review_capture_hashes'] = {m:c['sha256'] for m,c in by_id[int(row['id'])]['captures'].items()}
    from collections import Counter
    catalog['counts']['statuses'] = dict(Counter(r['status'] for r in catalog['scenes']))
    catalog['counts']['explicit_visual_decisions'] = len(decisions)
    catalog['status_policy']['reviewed'] = 'Explicit source/capture-bound visual acceptance; limitations allowed, exact parity not required.'
    catalog['status_policy']['blocked'] = 'Explicit needs-work decision or historical failed screening pending retest.'
    return catalog
