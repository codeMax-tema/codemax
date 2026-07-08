from __future__ import annotations

from dataclasses import dataclass, field
from hashlib import sha1


@dataclass(frozen=True, slots=True)
class AgentProposal:
    id: str
    title: str
    summary: str
    advantages: list[str] = field(default_factory=list)
    drawbacks: list[str] = field(default_factory=list)
    risks: list[str] = field(default_factory=list)
    impact: str = "medium"
    estimated_effort: str = "medium"
    recommended: bool = False
    rationale: str = ""


class ProposalService:
    def generate(self, title: str, description: str = "") -> list[AgentProposal]:
        return self._build(title, description, feedback="")

    def regenerate(
        self,
        title: str,
        feedback: str,
        previous: list[AgentProposal] | None = None,
    ) -> list[AgentProposal]:
        previous_count = len(previous or [])
        return self._build(title, "", feedback=f"{feedback}; previous={previous_count}")

    def _build(self, title: str, description: str, feedback: str) -> list[AgentProposal]:
        seed = _seed(title, description, feedback)
        feedback_note = f" Feedback preference: {feedback.lower()}." if feedback else ""
        return [
            AgentProposal(
                id=f"proposal-{seed}-incremental",
                title="Incremental safe path",
                summary="Make the smallest isolated change first, then expand after validation.",
                advantages=["Small diff", "Easy rollback", "Fast validation"],
                drawbacks=["May need follow-up cleanup"],
                risks=["Can leave temporary duplication"],
                impact="low",
                estimated_effort="low",
                recommended=True,
                rationale=f"Recommended because it protects the user's current workflow.{feedback_note}",
            ),
            AgentProposal(
                id=f"proposal-{seed}-modular",
                title="Modular boundary path",
                summary="Extract a clearer module boundary before changing behavior.",
                advantages=["Cleaner long-term design", "Improves testability"],
                drawbacks=["Larger diff", "Needs broader review"],
                risks=["Touches more files"],
                impact="medium",
                estimated_effort="medium",
                rationale=f"Useful when maintainability matters more than speed.{feedback_note}",
            ),
            AgentProposal(
                id=f"proposal-{seed}-parallel",
                title="Parallel comparison path",
                summary="Ask multiple models or strategies for candidates, then choose one.",
                advantages=["Better for ambiguous architecture", "Captures trade-offs"],
                drawbacks=["Higher cost", "Slower upfront"],
                risks=["Requires careful judge criteria"],
                impact="high",
                estimated_effort="high",
                rationale=f"Useful for high-risk or unclear tasks.{feedback_note}",
            ),
        ]


def _seed(*parts: str) -> str:
    text = "|".join(part.strip() for part in parts)
    return sha1(text.encode("utf-8")).hexdigest()[:10]
