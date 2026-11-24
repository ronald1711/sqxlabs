"""Stable local account context without importing or connecting to MT5."""
import hashlib
import json


def account_context(account):
    if account is None:
        raise ValueError('account metadata unavailable')
    login = getattr(account, 'login', None)
    if type(login) is not int or login <= 0:
        raise ValueError('positive account login required for capture identity')
    identity = {k: getattr(account, k) for k in ('server', 'currency', 'leverage', 'trade_mode')}
    # Pseudonymous identifier, not anonymization or authentication. Raw account
    # numbers need not appear in logs, manifests or recording status output.
    identity['account_fingerprint'] = hashlib.sha256(json.dumps(
        ['sqx-account-v1', identity['server'], login], separators=(',', ':')).encode()).hexdigest()
    return identity


