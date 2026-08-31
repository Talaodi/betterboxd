/** 控制台（默认页；AI 对话页的 global 实例）。UI 由共享 ChatPanel 渲染。 */
import ChatPanel from "../components/ChatPanel";

export default function Console() {
  return (
    <div className="flex h-full flex-col items-center px-6 pt-6">
      <div className="mb-6 flex w-full max-w-[760px] flex-1 flex-col">
        <ChatPanel />
      </div>
    </div>
  );
}
