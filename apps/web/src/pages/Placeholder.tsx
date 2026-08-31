/** 占位页（Lists/Chats/Stats/Reviews/设置 在后续里程碑点亮）。 */
export default function Placeholder({
  title,
  note,
}: {
  title: string;
  note: string;
}) {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center">
        <h1 className="text-xl text-[#8899aa]">{title}</h1>
        <p className="mt-2 text-sm text-[#5a6b7c]">{note}</p>
      </div>
    </div>
  );
}
