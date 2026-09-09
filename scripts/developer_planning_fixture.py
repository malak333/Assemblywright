"""Deterministic planning responses and owner actions for disposable native E2Es."""
import json
import time
import urllib.error
import urllib.request
import uuid


def planning_output(packet, digest):
    result = dict(schema_version=1, planning_packet_sha256=digest,
                  provider_id='openai.codex', model_id='gpt-5.6-sol', response_kind='question',
                  question=None, understanding_summary=[], assumptions=None, open_questions=[],
                  approaches=[], design_section=None, design_complete=False,
                  decision_log=[], implementation_plan=None)
    expected = packet['expected_response']
    if expected == 'question_or_understanding':
        if not packet['answers']:
            result['question'] = {'text': 'Should this feature preserve existing behavior outside the requested change?',
                                  'choices': ['Yes, preserve existing behavior', 'Clarify the scope']}
        else:
            result.update(response_kind='understanding', understanding_summary=[
                'Implement the requested feature in the selected project.',
                'Serve the project owner using the existing workflow.',
                'Preserve existing behavior outside the requested change.',
                'Use the configured validation command to verify the result.',
                'Do not introduce unrelated features or dependencies.'], assumptions={
                    'performance': 'Keep this small feature responsive.', 'scale': 'One project owner.',
                    'security_privacy': 'No new network access or credentials.',
                    'reliability_availability': 'Report failures explicitly and preserve existing files.',
                    'maintenance_ownership': 'The owner maintains the project; prefer existing conventions.', 'other': []})
    elif expected == 'approaches':
        result.update(response_kind='approaches', approaches=[
            {'id': 'minimal', 'title': 'Extend existing code', 'summary': 'Make the requested change using current conventions.',
             'tradeoffs': ['Small change with limited new dependencies.'], 'recommended': True},
            {'id': 'separate', 'title': 'Add a separate component', 'summary': 'Isolate the feature behind a new component.',
             'tradeoffs': ['More structure and maintenance for this small scope.'], 'recommended': False}])
    elif expected == 'design_section':
        result.update(response_kind='design_section', design_section={
            'id': 'implementation', 'title': 'Implementation and verification',
            'body': 'Implement the original request using the selected approach. Keep existing behavior and inputs intact. Handle errors explicitly. Run the configured validation command and request independent code review. Avoid unrelated dependencies and external services.'},
            decision_log=[{'decision': 'Use the owner-selected approach for the original request.',
                           'alternatives': ['Introduce a separate component.'],
                           'reason': 'Matches the confirmed scope and keeps maintenance small.'}])
    elif expected == 'design_section_or_ready':
        result.update(response_kind='ready', design_complete=True, decision_log=[
            {'decision': 'Use the owner-selected approach for the original request.',
             'alternatives': ['Introduce a separate component.'], 'reason': 'Matches the confirmed scope and keeps maintenance small.'}],
            implementation_plan='1. Read the existing project.\n2. Implement the original feature request and preserve existing behavior.\n3. Run the configured validation command.\n4. Address independent reviewer findings.')
    else:
        raise AssertionError('Unknown planning response stage: ' + expected)
    return result


def enqueue_with_plan(base_url, token, feature):
    """Exercise real planning/approval endpoints; never bypass product admission."""
    def api(path, body=None):
        request = urllib.request.Request(base_url + '/' + path,
            data=json.dumps(body).encode() if body is not None else None,
            headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
        return json.load(urllib.request.urlopen(request, timeout=10))

    feature_id = feature['id']
    # Preserve exact replay and mismatch checks against the actual control route.
    current = api('status')
    if any(item['id'] == feature_id for item in current['queue']):
        return api('control', dict(action='enqueue', **feature))

    def mutate(action, state=None, **values):
        body = dict(action=action, feature_id=feature_id, request_id=str(uuid.uuid4()),
                    expected_revision=state['revision'] if state else 0, **values)
        return api('planning', body)

    state = mutate('start', project=feature['project'], instruction=feature['instruction'],
                   validation=feature['validation'], model_target=feature.get('model_target', 'mac'))
    for _ in range(40):
        deadline = time.monotonic() + 45
        while state['running'] and time.monotonic() < deadline:
            time.sleep(.05)
            state = api('planning?id=' + feature_id)
        assert not state['running'] and state['availability'] == 'available', state
        stage = state['stage']
        if stage == 'questions':
            state = mutate('answer', state, answer='Yes. Preserve existing behavior and implement only the requested feature.')
        elif stage == 'understanding':
            assert not state['open_questions'], state
            state = mutate('confirm_understanding', state)
        elif stage == 'approaches':
            approach = next(a for a in state['approaches'] if a['recommended'])
            state = mutate('select_approach', state, approach_id=approach['id'])
        elif stage == 'design':
            state = mutate('confirm_design', state)
        elif stage == 'ready':
            assert state['documents']['plan_sha256'], state
            state = mutate('approve_and_enqueue', state)
        elif stage == 'enqueued':
            return api('status')
        else:
            raise AssertionError(state)
    raise AssertionError('Planning did not reach approved enqueue')
