"""CSV 输出"""

import csv
from typing import Any, Dict, List

from code_stats.outputs.base import BaseOutput


class CsvOutput(BaseOutput):
    """CSV 输出器"""

    def output(
        self,
        all_repos: List[Dict[str, Any]],
        summary: Dict[str, Any]
    ) -> None:
        """输出 CSV 格式"""
        output_file = self.output_file or 'code_statistics.csv'

        with open(output_file, 'w', newline='', encoding='utf-8') as f:
            writer = csv.writer(f)
            writer.writerow([
                'Repository', 'Language', 'Code_Files', 'Code_Lines',
                'Doc_Files', 'Doc_Lines', 'Code_Size_MB', 'Total_Size_MB'
            ])

            for repo in all_repos:
                code_size_mb = round(repo.get('size', 0) / (1024 * 1024), 2)
                total_size_mb = round(repo.get('total_size', 0) / (1024 * 1024), 2)
                writer.writerow([
                    repo['name'],
                    repo['language'],
                    repo['files'],
                    repo['lines'],
                    repo.get('doc_files', 0),
                    repo.get('doc_lines', 0),
                    code_size_mb,
                    total_size_mb
                ])

            writer.writerow([])
            writer.writerow(['Summary'])
            writer.writerow(['Total Repos', summary['total_repos']])
            writer.writerow(['Total Code Files', summary['total_files']])
            writer.writerow(['Total Code Lines', summary['total_lines']])
            writer.writerow(['Total Doc Files', summary.get('total_doc_files', 0)])
            writer.writerow(['Total Doc Lines', summary.get('total_doc_lines', 0)])
            writer.writerow([
                'Total Code Size (MB)',
                round(summary.get('total_size', 0) / (1024 * 1024), 2)
            ])
            writer.writerow([
                'Total Repository Size (MB)',
                round(summary.get('total_all_size', 0) / (1024 * 1024), 2)
            ])

        self._print_saved_message(output_file)
