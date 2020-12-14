project = 'hardshare'
copyright = '2018-2020 rerobots, Inc. | <a href="https://github.com/rerobots/hardshare/">source code</a>'
author = 'rerobots, Inc.'
html_logo = '_static/logo.svg'

import os.path
import sys
sys.path.insert(0, os.path.join(os.getcwd(), '..', 'py-ws'))
from setup import version
release = version

extensions = [
    'sphinx.ext.mathjax',
]

source_suffx = '.rst'
exclude_patterns = ['_build']
master_doc = 'index'

language = None
pygments_style = 'sphinx'
html_theme = 'alabaster'
html_static_path = ['_static']
htmlhelp_basename = 'hardsharedoc'


# Prepare to build on hosts of https://readthedocs.org/
import os
if os.environ.get('READTHEDOCS', 'False') == 'True':
    import subprocess
    subprocess.check_call('./get-deps.sh')
