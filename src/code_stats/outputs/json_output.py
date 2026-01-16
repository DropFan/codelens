"""JSON 输出"""

import json
from datetime import datetime
from typing import Any, Dict, List

from code_stats.outputs.base import BaseOutput


class JsonOutput(BaseOutput):
    """JSON 输出器"""

    def output(
        self,
        all_repos: List[Dict[str, Any]],
        summary: Dict[str, Any]
    ) -> None:
        """输出 JSON 格式"""
        result = {
            'generated_at': datetime.now().isoformat(),
            'summary': summary,
            'repositories': all_repos
        }

        output_file = self.output_file or 'code_statistics.json'
        with open(output_file, 'w', encoding='utf-8') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)

        self._print_saved_message(output_file)
