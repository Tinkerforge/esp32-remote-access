
export function create_response(body, init) {
    const decoder = new TextDecoder();
    const text = decoder.decode(body);
    console.log("Text:", text);
    return new Response(text, init);
}
