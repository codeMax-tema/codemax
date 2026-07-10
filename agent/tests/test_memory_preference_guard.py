import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.memory import MemoryService, NewConversationMessage


class MemoryPreferenceGuardTests(unittest.TestCase):
    def test_preference_messages_do_not_auto_persist_long_term_memory(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            service = MemoryService(root=Path(temp_dir))
            service.add_message(
                NewConversationMessage(
                    conversationId="conv-1",
                    role="user",
                    content="Please remember that I prefer concise commit messages.",
                    repositoryPath="D:/repo",
                )
            )

            persisted = service.retrieve_memories(repository_path="D:/repo")
            extracted = service.extract_memories(
                conversation_id="conv-1",
                repository_path="D:/repo",
            )
            persisted_after_extract = service.retrieve_memories(repository_path="D:/repo")

        self.assertEqual(persisted, [])
        self.assertTrue(any(memory.category == "preference" for memory in extracted))
        self.assertEqual(persisted_after_extract, [])

    def test_non_preference_memories_still_persist_from_messages(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            service = MemoryService(root=Path(temp_dir))
            service.add_message(
                NewConversationMessage(
                    conversationId="conv-2",
                    role="user",
                    content="Use `pnpm test --filter desktop` before merging.",
                    repositoryPath="D:/repo",
                )
            )

            persisted = service.retrieve_memories(repository_path="D:/repo")

        self.assertTrue(any(memory.category == "repository_command" for memory in persisted))


if __name__ == "__main__":
    unittest.main()
